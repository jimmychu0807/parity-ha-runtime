use support::{ decl_module, decl_storage, decl_event, dispatch::Result,
  StorageValue, StorageMap, ensure, traits::{ Currency, ReservableCurrency } };
use { system::ensure_signed, timestamp };

// this is needed when you want to use Vec and Box
use rstd::prelude::*;
use runtime_primitives::traits::{ As, /*CheckedAdd, CheckedDiv, CheckedMul,*/ Hash };
use parity_codec::{ Encode, Decode };
// use runtime_io::{ self };

pub type StdResult<T> = rstd::result::Result<T, &'static str>;

/// The module's configuration trait. This is trait inheritance.
pub trait Trait: timestamp::Trait + balances::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// store the 3 topmost bids, and they cannot be withdrawn
const TOPMOST_BIDS_LEN: usize = 3;
// auction duration has to be at least 3 mins
const AUCTION_MIN_DURATION: u64 = 3 * 60;
// modify the following to at least 1 min when run in production
const DISPLAY_BIDS_UPDATE_PERIOD: u64 = 1 * 60;

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum AuctionStatus {
  Ongoing,
  Cancelled,
  Closed
}
// necessary so structs depending on this enum can be en-/de-code with
//   default value.
impl Default for AuctionStatus {
  fn default() -> Self { AuctionStatus::Ongoing }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum BidStatus {
  Active,
  Withdrawn,
}
// necessary so structs depending on this enum can be en-/de-code with
//   default value.
impl Default for BidStatus {
  fn default() -> Self { BidStatus::Active }
}

// Our own Cat struct
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Kitty<Hash, AccountId> {
  id: Hash,
  name: Option<Vec<u8>>,
  owner: Option<AccountId>,
  owner_pos: Option<u64>,
  in_auction: bool,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Auction<Hash, Balance, Moment, AuctionTx> {
  id: Hash,
  kitty_id: Hash,
  base_price: Balance,
  start_time: Moment,
  end_time: Moment,
  status: AuctionStatus,

  topmost_bids: Vec<Hash>,
  price_to_topmost: Balance,
  display_bids: Vec<Hash>,
  display_bids_last_update: Moment,

  tx: Option<AuctionTx>,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Bid<Hash, AccountId, Balance, Moment> {
  id: Hash,
  auction_id: Hash,
  bidder: AccountId,
  price: Balance,
  last_update: Moment,
  status: BidStatus,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct AuctionTx<Moment, AccountId, Balance> {
  tx_time: Moment,
  winner: AccountId,
  tx_price: Balance,
}

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as CatAuction {
    Kitties get(kitties): map T::Hash => Kitty<T::Hash, T::AccountId>;
    KittiesArray get(kitty_array): map u64 => T::Hash;
    KittiesCount get(kitties_count): u64 = 0;

    // The following two go hand-in-hand, write to one likely need to update the other two
    OwnerKitties get(owner_kitties): map (T::AccountId, u64) => T::Hash;
    OwnerKittiesCount get(owner_kitties_count): map T::AccountId => u64 = 0;

    // On Auction
    Auctions get(auctions): map T::Hash => Auction<T::Hash, T::Balance, T::Moment,
      AuctionTx<T::Moment, T::AccountId, T::Balance>>;
    AuctionsArray get(auction_array): map u64 => T::Hash;
    AuctionsCount get(auctions_count): u64 = 0;

    // `bid_id` => Bid object
    Bids get(bids): map T::Hash => Bid<T::Hash, T::AccountId, T::Balance, T::Moment>;

    // On auction & bid: (auction_id, index) => bid_id
    AuctionBids get(auction_bids): map (T::Hash, u64) => T::Hash;
    AuctionBidsCount get(auction_bids_count): map T::Hash => u64 = 0;
    AuctionBidderBids get(auction_bidder_bids): map (T::Hash, T::AccountId) => T::Hash;

    Nonce: u64 = 0;
  }
}

decl_event!(
  pub enum Event<T> where
    <T as system::Trait>::AccountId,
    <T as system::Trait>::Hash,
    <T as balances::Trait>::Balance,
    <T as timestamp::Trait>::Moment {

    // Events in our runtime
    KittyCreated(AccountId, Hash, Vec<u8>),
    AuctionStarted(AccountId, Hash, Hash, Balance, Moment),
    AuctionCancelled(Hash),
    AuctionClosed(Hash),
    NewBid(Hash, Balance),
    UpdateDisplayedBids(Hash, Vec<Hash>),
    AuctionTx(Hash, Hash, AccountId, AccountId),
  }
);

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    fn deposit_event<T>() = default;

    pub fn create_kitty(origin, kitty_name: Vec<u8>) -> Result {
      let sender = ensure_signed(origin)?;

      let kitty_id = Self::_gen_random_hash(&sender)?;
      // ensure the kitty_id is not existed
      ensure!(!<Kitties<T>>::exists(&kitty_id), "Cat with the id existed already");

      let mut kitty = Kitty {
        id: kitty_id,
        name: Some(kitty_name.clone()),
        owner: None,      // to be updated in _add_kitty_to_storage
        owner_pos: None,  // to be updated in _add_kitty_to_storage
        in_auction: false,
      };
      Self::_add_kitty_to_storage(&mut kitty, Some(&sender))?;

      // emit an event
      Self::deposit_event(RawEvent::KittyCreated(sender, kitty_id, kitty_name));
      Ok(())
    } // end of fn `create_kitty`

    pub fn start_auction(origin, kitty_id: T::Hash, end_time: T::Moment,
      base_price: T::Balance) -> Result {

      let sender = ensure_signed(origin)?;
      // Check:
      //  1. ensure kitty exists, and the kitty.owner == sender. Currently,
      //     only the kitty owner can put his own kitty in auction
      //  2. kitty is not already `in_auction` state
      //  3. ensure end_time > current_time
      //  4. base_price > 0

      // check #1
      ensure!(<Kitties<T>>::exists(kitty_id), "Kitty does not exist");
      let kitty = Self::kitties(kitty_id);

      // check #2
      ensure!(!kitty.in_auction, "Kitty is already in another auction");

      // check #3
      let now = <timestamp::Module<T>>::get();
      ensure!(end_time.clone().as_() > AUCTION_MIN_DURATION + now.clone().as_(),
        "End time cannot be set less than 3 mins from current time");

      // check #4
      ensure!(base_price > <T::Balance as As<u64>>::sa(0),
        "Base price must be set greater than 0");

      // Write:
      //  1. create the auction
      let auction_id = Self::_gen_random_hash(&sender)?;
      // check: auction_id not existed yet
      ensure!(!<Auctions<T>>::exists(&auction_id), "Auction ID generated exists already");

      let auction = Auction {
        id: auction_id.clone(),
        kitty_id,
        base_price,
        start_time: now.clone(),
        end_time: end_time.clone(),
        status: AuctionStatus::Ongoing,

        topmost_bids: Vec::new(),
        price_to_topmost: base_price,
        display_bids: Vec::new(),
        display_bids_last_update: now,

        tx: None,
      };

      Self::_add_auction_to_storage(&auction)?;

      // also set the kitty state in_auction = true
      <Kitties<T>>::mutate(kitty_id, |k| k.in_auction = true);

      // emit an event
      Self::deposit_event(RawEvent::AuctionStarted(sender, kitty_id, auction_id,
        base_price, end_time));
      Ok(())
    } // end of `fn start_auction(...)

    pub fn cancel_auction(origin, auction_id: T::Hash) -> Result {

      let sender = ensure_signed(origin)?;

      // check:
      //   1. only the auction_admin (which is the kitty owner) can cancel the auction
      //   2. the current time is before the auction end time
      //   3. No one has placed bid in the auction yet

      // check #1:
      ensure!(<Auctions<T>>::exists(auction_id), "Auction does not exist");
      ensure!(Self::_auction_admin(auction_id) == sender, "You are not the auction admin");

      let auction = Self::auctions(auction_id);
      let kitty_id = auction.kitty_id;
      let now = <timestamp::Module<T>>::get();
      // check #2:
      ensure!(auction.end_time > now, "The auction has passed its end time");

      // check #3:
      ensure!(Self::auction_bids_count(auction_id) == 0,
        "Someone has bidded already. So this auction cannot be cancelled");

      // write:
      //   1. update the auction status to cancelled.
      //   2. update the cat status
      <Auctions<T>>::mutate(auction_id, |auction| auction.status = AuctionStatus::Cancelled);
      <Kitties<T>>::mutate(kitty_id, |kitty| kitty.in_auction = false);

      Self::deposit_event(RawEvent::AuctionCancelled(auction_id));
      Ok(())
    } // end of `fn cancel_auction(...)`

    pub fn bid(origin, auction_id: T::Hash, bid_price: T::Balance) -> Result {

      let bidder = ensure_signed(origin)?;
      // check:
      //   1. bidder is not the kitty owner
      //   2. bid_price >= base_price
      //   3. check the auction status is still ongoing
      //   4. now < auction end_time

      // check #1
      ensure!(<Auctions<T>>::exists(auction_id), "Auction does not exist");
      let auction = Self::auctions(auction_id);
      let kitty_owner = Self::kitties(auction.kitty_id).owner.ok_or("Kitty does not have owner")?;
      ensure!(bidder != kitty_owner, "The kitty owner cannot bid in this auction");

      // check #2
      ensure!(bid_price >= auction.base_price, "The bid price is lower than the auction base price");

      // check #3
      ensure!(auction.status == AuctionStatus::Ongoing, "Auction is not active");

      // check #4
      let now = <timestamp::Module<T>>::get();
      ensure!(now < auction.end_time, "Auction has expired already");

      //write #1
      let to_reserve: T::Balance;
      let bid = if <AuctionBidderBids<T>>::exists((auction_id, bidder.clone())) {

        // Overwriting on his own previous bid

        let bid = Self::bids(Self::auction_bidder_bids((auction_id, bidder.clone())));
        // check the current bid is larger than its previous bid
        ensure!(bid_price > bid.price, "New bid has to be larger than your previous bid");

        to_reserve = bid_price - bid.price;  // only reserve the difference from his previous bid
        <Bids<T>>::mutate(bid.id, |bid| {
          bid.price = bid_price;
          bid.last_update = now;
        });

        bid // bid returned
      } else {

        // This is a new bid for this bidder

        let bid = Bid {
          id: Self::_gen_random_hash(&bidder)?,
          auction_id,
          bidder: bidder.clone(),
          price: bid_price,
          last_update: now,
          status: BidStatus::Active
        };

        // check the bid ID is a new unique ID
        ensure!(!<Bids<T>>::exists(&bid.id), "Generated bid ID is duplicated");

        // add into storage
        <Bids<T>>::insert(bid.id, bid.clone());
        <AuctionBids<T>>::insert((auction_id, Self::auction_bids_count(auction_id)),
          bid.id);
        <AuctionBidsCount<T>>::mutate(auction_id, |cnt| *cnt += 1);
        <AuctionBidderBids<T>>::insert((auction_id, bidder.clone()), bid.id);
        to_reserve = bid_price;

        bid  // bid returned
      };

      // bidder money has to be locked here
      <balances::Module<T>>::reserve(&bidder, to_reserve)?;

      // update auction bid info inside if higher than topmost
      if bid_price >= auction.price_to_topmost {
        let _ = Self::_update_auction_topmost_bids(&auction_id, &bid.id);
      }

      // emit an event
      Self::deposit_event(RawEvent::NewBid(auction_id, bid_price));

      Ok(())
    }

    pub fn update_auction_display_bids(_origin, auction_id: T::Hash) -> Result {
      // no need to verify caller, anyone can call this method

      // check:
      //   1. auction existed
      //   2. auction is still ongoing
      //   3. its last updated time passed the DISPLAY_BIDS_UPDATE_PERIOD
      ensure!(<Auctions<T>>::exists(auction_id), "The auction does not exist");
      let now = <timestamp::Module<T>>::get();
      let auction = Self::auctions(auction_id);
      let to_update = DISPLAY_BIDS_UPDATE_PERIOD + auction.display_bids_last_update.as_();

      ensure!(auction.status == AuctionStatus::Ongoing, "The auction is no longer running.");
      ensure!(to_update <= now.clone().as_(), "The auction display bids has just been recently updated.");

      Self::_update_auction_display_bids_nocheck(auction_id, true)
    }

    pub fn close_auction_and_tx(_origin, auction_id: T::Hash) -> Result {
      ensure!(<Auctions<T>>::exists(auction_id), "The auction does not exist");
      let now = <timestamp::Module<T>>::get();
      let auction = Self::auctions(auction_id);

      ensure!(auction.status == AuctionStatus::Ongoing, "The auction is no longer running.");
      ensure!(now >= auction.end_time, "The auction is not expired yet.");

      // write
      //   1. check if there is a highest bidder. If yes
      //     - unreserve his money,
      //     - transfer his money to kitty_owner
      //     - update kitty to the bidder
      //     - emit an event saying an auction with aid has a transaction, of kitty_id
      //       from AccountId to AccountId
      //   2. unreserve all fund from the rest of the bidders
      //   3. set auction status to Closed
      //     - emit an event saying auction closed

      // #1. Transact the kitty and money between winner and kitty owner
      let mut winner_opt: Option<T::AccountId> = None;
      let mut auction_tx_opt: Option<AuctionTx<T::Moment, T::AccountId, T::Balance>> = None;

      if auction.topmost_bids.len() > 0 {
        let reward_bid = Self::bids(auction.topmost_bids[0]);
        winner_opt = Some(reward_bid.bidder.clone());
        let kitty_owner = Self::kitties(auction.kitty_id).owner.unwrap();

        // 1) unreserve winner money,
        // 2) transfer winner money to kitty_owner,
        // 3) transfer kitty ownership to the winner
        if let Some(ref winner_ref) = winner_opt {
          <balances::Module<T>>::unreserve(winner_ref, reward_bid.price);
          let _transfer = <balances::Module<T> as Currency<_>>::transfer(winner_ref, &kitty_owner, reward_bid.price);
          match _transfer {
            Err(_e) => Err("Fund transfer error"),
            Ok(_v) => {
              Self::_transfer_kitty_ownership(&auction.kitty_id, winner_ref);

              // create the auction_tx here
              auction_tx_opt = Some(AuctionTx {
                tx_time: now,
                winner: winner_ref.clone(),
                tx_price: reward_bid.price
              });

              // emit event of the kitty is transferred
              Self::deposit_event(RawEvent::AuctionTx(auction_id, auction.kitty_id, kitty_owner, winner_opt.clone().unwrap()));
              Ok(())
            },
          }?;
        }
      } else {
        // No one bid. So no kitty ownership transfer is made. Resume the kitty to the owner
        <Kitties<T>>::mutate(Self::auctions(auction_id).kitty_id, |kitty| {
          kitty.in_auction = false;
        });
      }

      // #2. unreserve funds for other bidders
      let bids_count = <AuctionBidsCount<T>>::get(auction_id);
      (0..bids_count)
        .map(|i| Self::bids( Self::auction_bids((auction_id, i)) ) )  // get the bids
        .filter(|bid| match &winner_opt {                             // filter out the auction winner
          Some(winner) => *winner != bid.bidder,
          None => true
        })
        .for_each(|bid| {                                             // unreserve funds for other bidders
          <balances::Module<T>>::unreserve(&bid.bidder, bid.price);
        });

      // #3. close the auction and emit event
      <Auctions<T>>::mutate(auction_id, |auction| {
        auction.status = AuctionStatus::Closed;
        auction.tx = auction_tx_opt;
      });

      // #4. update the display bid upon closing
      let _ = Self::_update_auction_display_bids_nocheck(auction_id, false);

      Self::deposit_event(RawEvent::AuctionClosed(auction_id));

      Ok(())
    }

  } // end of `struct Module<T: Trait> for enum Call...`
} // end of `decl_module!`

impl<T: Trait> Module<T> {
  // generate a random hash key
  fn _gen_random_hash(sender: &T::AccountId) -> StdResult<T::Hash> {
    let nonce = <Nonce<T>>::get();
    let random_seed = <system::Module<T>>::random_seed();
    let random_hash = (random_seed, sender, nonce).using_encoded(<T as system::Trait>::Hashing::hash);

    // nonce increment by 1
    <Nonce<T>>::mutate(|nonce| *nonce += 1);

    Ok(random_hash)
  }

  // allow owner to be None
  fn _add_kitty_to_storage(kitty: &mut Kitty<T::Hash, T::AccountId>, owner: Option<&T::AccountId>)
    -> Result
  {
    let kitty_id: T::Hash = kitty.id;

    // add the owner reference if `owner` is specified
    if let Some(owner_id) = owner {
      kitty.owner = Some(owner_id.clone());
      kitty.owner_pos = Some(Self::owner_kitties_count(owner_id));

      // update OwnerKitties storage...
      <OwnerKitties<T>>::insert((owner_id.clone(), kitty.owner_pos.unwrap()), &kitty_id);
      <OwnerKittiesCount<T>>::mutate(owner_id, |cnt| *cnt += 1);
    }

    // update kitty-related storages
    <Kitties<T>>::insert(&kitty_id, kitty.clone());
    <KittiesArray<T>>::insert(Self::kitties_count(), &kitty_id);
    <KittiesCount<T>>::mutate(|cnt| *cnt += 1);

    Ok(())
  }

  fn _add_auction_to_storage(auction: &Auction<T::Hash, T::Balance,
    T::Moment, AuctionTx<T::Moment, T::AccountId, T::Balance>>) -> Result
  {
    <Auctions<T>>::insert(auction.id, auction);
    <AuctionsArray<T>>::insert(Self::auctions_count(), auction.id);
    <AuctionsCount<T>>::mutate(|cnt| *cnt += 1);

    Ok(())
  }

  fn _auction_admin(auction_id: T::Hash) -> T::AccountId {
    // we use an internal function here, so later on we can modify the logic
    //   how an auction admin is determined.

    let auction = Self::auctions(auction_id);
    let kitty = Self::kitties(auction.kitty_id);
    kitty.owner.unwrap()
  }

  fn _update_auction_topmost_bids(auction_id: &T::Hash, bid_id: &T::Hash) -> Result {
    let auction = Self::auctions(auction_id);
    let bid = Self::bids(bid_id);

    if bid.price < auction.price_to_topmost {
      return Ok(());
    }

    <Auctions<T>>::mutate(auction_id, |auction| {
      // it could be this bid is a topmost bid already with bid_price being updated
      if !auction.topmost_bids.contains(bid_id) {
        auction.topmost_bids.push(bid_id.clone());
      }

      // sort the bids
      auction.topmost_bids.sort_by(|a, b| {
        let a_bp = Self::bids(a).price;
        let b_bp = Self::bids(b).price;
        b_bp.partial_cmp(&a_bp).unwrap()
      });

      // drop the last bid if needed
      auction.topmost_bids = auction.topmost_bids.clone()
        .into_iter().take(TOPMOST_BIDS_LEN).collect();

      // update the price_to_topmost. Only update it when the vector is filled
      if auction.topmost_bids.len() >= TOPMOST_BIDS_LEN {
        let bid = Self::bids(auction.topmost_bids[TOPMOST_BIDS_LEN - 1]);
        auction.price_to_topmost = bid.price + <T::Balance as As<u64>>::sa(1);
      }
    });

    Ok(())
  }

  fn _update_auction_display_bids_nocheck(auction_id: T::Hash, ev: bool) -> Result {
    let now = <timestamp::Module<T>>::get();

    <Auctions<T>>::mutate(auction_id, |auction| {
      auction.display_bids = auction.topmost_bids.clone();
      auction.display_bids_last_update = now.clone();
    });
    // emit event depends on the passed-in flag
    if ev {
      let auction = Self::auctions(auction_id);
      Self::deposit_event(RawEvent::UpdateDisplayedBids(auction_id, auction.display_bids));
    }

    Ok(())
  }

  fn _transfer_kitty_ownership(kitty_id: &T::Hash, new_owner_ref: &T::AccountId) {
    // Need to update:
    //   1. update OwnerKitties, OwnerKittiesCount of original owner
    //   2. update OwnerKitties, OwnerKittiesCount of new_owner
    //   3. update Kitty (owner, owner_pos)
    let kitty = Self::kitties(kitty_id);

    // 1. update OwnerKitties, OwnerKittiesCount of original owner
    let orig_kitty_owner = kitty.owner.clone().unwrap();
    let kitty_cnt = Self::owner_kitties_count(&orig_kitty_owner);
    let kitty_owner_pos = kitty.owner_pos.unwrap();

    // Two cases: when 1) the kitty is at the last position in OwnerKitties storage, 2) or not
    if kitty_owner_pos == kitty_cnt - 1 {
      // transferred kitty is at the last position, just need to remove that from OwnerKitties
      <OwnerKitties<T>>::remove((orig_kitty_owner.clone(), kitty_cnt - 1));
    } else {
      // we move the kitty in the last position to the position of the transferring kitty
      let last_kitty_id = Self::owner_kitties((orig_kitty_owner.clone(), kitty_cnt - 1));

      // update the kitty storage value
      <Kitties<T>>::mutate(last_kitty_id, |last_kitty| last_kitty.owner_pos = Some(kitty_owner_pos));

      // switch kitty position here
      <OwnerKitties<T>>::remove((orig_kitty_owner.clone(), kitty_cnt - 1));
      <OwnerKitties<T>>::insert(
        (orig_kitty_owner.clone(), kitty_owner_pos),
        last_kitty_id
      );
    }
    <OwnerKittiesCount<T>>::mutate(&orig_kitty_owner, |cnt| *cnt -= 1);

    // 2. update OwnerKitties, OwnerKittiesCount of new_owner
    let kitty_new_pos = Self::owner_kitties_count(new_owner_ref);
    <OwnerKitties<T>>::insert((new_owner_ref.clone(), kitty_new_pos), kitty_id);
    <OwnerKittiesCount<T>>::mutate(new_owner_ref, |cnt| *cnt += 1);

    // 3. update the kitty
    <Kitties<T>>::mutate(kitty_id, |kitty| {
      kitty.owner = Some(new_owner_ref.clone());
      kitty.owner_pos = Some(kitty_new_pos);
      kitty.in_auction = false;
    });
  }
}

#[cfg(test)]
mod tests {
  // Test Codes
  use super::*;
  use support::{ impl_outer_origin, assert_ok };
  use runtime_io::{ with_externalities, TestExternalities };
  use primitives::{ H256, Blake2Hasher };
  use runtime_primitives::{
    BuildStorage, traits::{BlakeTwo256, IdentityLookup},
    testing::{Digest, DigestItem, Header}
  };

  // Manually called this which is called in `contstruct_runtime`
  impl_outer_origin! {
    pub enum Origin for CatAuctionTest {}
  }

  #[derive(Clone, Eq, PartialEq)]
  pub struct CatAuctionTest;

  impl system::Trait for CatAuctionTest {
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Digest = Digest;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = ();
    type Log = DigestItem;
  }

  impl balances::Trait for CatAuctionTest {
    type Balance = u64;
    type OnFreeBalanceZero = ();
    type OnNewAccount = ();
    type Event = ();
    type TransactionPayment = ();
    type DustRemoval = ();
    type TransferPayment = ();
  }

  impl timestamp::Trait for CatAuctionTest {
    /// A timestamp: seconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = ();
  }

  impl super::Trait for CatAuctionTest {
    type Event = ();
  }

  type CatAuction = super::Module<CatAuctionTest>;

  const KITTY_NAMES: [&'static str; 3] = [
    "lovely-kitty01",
    "lovely-kitty02",
    "lovely-kitty03",
  ];

  const ALICE: u64 = 10;
  const BOB: u64 = 20;
  const CHARLES: u64 = 30;
  const DAVE: u64 = 40;
  const EVE: u64 = 50;

  const BASE_PRICE: u64 = 10000;
  const INI_BALANCE: u64 = 100000;

  // construct genesis storage
  fn build_ext() -> TestExternalities<Blake2Hasher> {
    let mut t = system::GenesisConfig::<CatAuctionTest>::default().build_storage().unwrap().0;

    t.extend(balances::GenesisConfig::<CatAuctionTest> {
      // this is where you specify the genesis data structure
      balances: vec![
        (ALICE, INI_BALANCE), (BOB, INI_BALANCE), (CHARLES, INI_BALANCE),
        (DAVE, INI_BALANCE), (EVE, INI_BALANCE) ],
      ..Default::default()
    }.build_storage().unwrap().0);

    t.into()
  }

  #[test]
  fn it_works() {
    // Test case to test all test mocks are setup properly
    with_externalities(&mut build_ext(), || {
      assert!(true);
    })
  } // finish test `it_works`

  #[test]
  fn can_create_kitty() {
    with_externalities(&mut build_ext(), || {
      let kitty_name_in_hex = KITTY_NAMES[0].as_bytes().to_vec();
      assert_ok!(CatAuction::create_kitty(Origin::signed(ALICE), kitty_name_in_hex));

      assert_eq!(CatAuction::kitties_count(), 1);
      assert_eq!(CatAuction::owner_kitties_count(ALICE), 1);

      let kitty_id = CatAuction::kitty_array(0);
      assert_eq!(CatAuction::owner_kitties((ALICE, 0)), kitty_id);

      // test kitty object data is consistent
      let kitty = CatAuction::kitties(kitty_id);
      assert_eq!(kitty.in_auction, false);
      assert_eq!(kitty.owner, Some(ALICE));
      assert_eq!(kitty.owner_pos, Some(0));
    })
  } // finish test `can_start_auction`

  #[test]
  fn can_start_auction_n_bid_n_close() {
    with_externalities(&mut build_ext(), || {
      let kitty_name_in_hex = KITTY_NAMES[0].as_bytes().to_vec();
      assert_ok!(CatAuction::create_kitty(Origin::signed(ALICE), kitty_name_in_hex));

      let kitty_id = CatAuction::kitty_array(0);
      let time_buffer = 5; // 5s for time buffer
      let end_time = <timestamp::Module<CatAuctionTest>>::get() +
        AUCTION_MIN_DURATION + time_buffer;

      assert_ok!(CatAuction::start_auction(Origin::signed(ALICE), kitty_id, end_time, BASE_PRICE));

      // Test auction:
      //   1. auctions_count
      assert_eq!(CatAuction::auctions_count(), 1);

      let auction_id = CatAuction::auction_array(0);
      let abal_b4_bid = <balances::Module<CatAuctionTest>>::free_balance(ALICE);
      let bbal_b4_bid = <balances::Module<CatAuctionTest>>::free_balance(BOB);

      // Bob bids in the auction
      assert_ok!(CatAuction::bid(Origin::signed(BOB), auction_id, BASE_PRICE));
      <timestamp::Module<CatAuctionTest>>::set_timestamp(end_time);

      // Close the auction
      assert_ok!(CatAuction::close_auction_and_tx(Origin::INHERENT, auction_id));

      // Check
      //   1. auction object
      //   2. payment is transferred
      //   3. kitty object
      //   4. OwnerKittiesCount (A, B)
      //   5. OwnerKitties (A, B)

      // check #1: auction object
      let auction = CatAuction::auctions(auction_id);
      assert_eq!(auction.status, AuctionStatus::Closed);
      let auction_tx = auction.tx.unwrap();
      assert_eq!(auction_tx.winner, BOB);
      assert_eq!(auction_tx.tx_price, BASE_PRICE);

      // check #2: payment is transferred
      let abal_after_bid = <balances::Module<CatAuctionTest>>::free_balance(ALICE);
      let bbal_after_bid = <balances::Module<CatAuctionTest>>::free_balance(BOB);
      assert!(abal_after_bid - abal_b4_bid >= BASE_PRICE);
      assert!(bbal_b4_bid - bbal_after_bid >= BASE_PRICE);

      // check #3: check kitty object
      let kitty = CatAuction::kitties(kitty_id);
      assert!(!kitty.in_auction);
      assert_eq!(kitty.owner, Some(BOB));
      assert_eq!(kitty.owner_pos, Some(0));

      // check #4: check OwnerKittiesCount
      assert_eq!(CatAuction::owner_kitties_count(ALICE), 0);
      assert_eq!(CatAuction::owner_kitties_count(BOB), 1);

      // check #5: check OwnerKitties
      assert!(!<OwnerKitties<CatAuctionTest>>::exists((ALICE, 0)));
      assert!(<OwnerKitties<CatAuctionTest>>::exists((BOB, 0)));
      assert_eq!(CatAuction::owner_kitties((BOB, 0)), kitty_id);
    });
  }

  // TODO: Write test cases:
  //   1. with alice, bob having more than one kitten, and in auction to test
  //      the kitty switching logic when auction closes and tx happens
  //   2. with alice starting an auction, Bob, Charles, Dave, and Eve come bid
  //      with each one out-bidding each others, and then auction closed.
}
