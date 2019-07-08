/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references

/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

use support::{decl_module, decl_storage, decl_event, dispatch::Result,
  StorageValue, StorageMap, ensure };
use { system::ensure_signed, timestamp };

// this is needed when you want to use Vec and Box
use rstd::prelude::*;
// use runtime_io;
use runtime_primitives::traits::{ As, CheckedAdd, CheckedDiv, CheckedMul, Hash };
use parity_codec::{ Encode, Decode };

pub type StdResult<T> = rstd::result::Result<T, &'static str>;

/// The module's configuration trait. This is trait inheritance.
pub trait Trait: timestamp::Trait + balances::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

// store the 3 topmost bids, and they cannot be withdrawn
const TOPMOST_BIDS_LEN: usize = 3;

// auction duration has to be at least 5 mins
// const AUCTION_MIN_DURATION: u64 = 5 * 60;

// Our own Cat struct
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Kitty<Hash, AccountId> {
  id: Hash,
  name: Option<Vec<u8>>,
  owner: Option<AccountId>,
  owner_pos: Option<u64>,
  in_auction: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum AuctionStatus {
  Ongoing,
  Cancelled,
  ToBeClaimed,
  Closed
}

// This is necessary so that other structs depend on this enum can be encode/decode with default value.
impl Default for AuctionStatus {
  fn default() -> Self { AuctionStatus::Ongoing }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Auction<Hash, Balance, Moment, AuctionTx> {
  id: Hash,
  kitty_id: Hash,
  base_price: Balance,
  start_time: Moment,
  end_time: Moment,
  status: AuctionStatus,

  current_topmost_bids: Vec<Hash>,
  bid_price_to_topmost: Balance,
  displayed_topmost_bids: Vec<Hash>,

  tx: Option<AuctionTx>,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum BidStatus {
  Active,
  Withdrawn,
}

// This is necessary so that other structs depend on this enum can be encode/decode with default value.
impl Default for BidStatus {
  fn default() -> Self { BidStatus::Active }
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
pub struct AuctionTx<Hash, AccountId, Balance, Moment> {
  auction_id: Hash,
  tx_time: Moment,
  buyer: AccountId,
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
      AuctionTx<T::Hash, T::AccountId, T::Balance, T::Moment>>;
    AuctionsArray get(auction_array): map u64 => T::Hash;
    AuctionsCount get(auctions_count): u64 = 0;

    // `bid_id` => Bid object
    Bids get(bids): map T::Hash => Bid<T::Hash, T::AccountId, T::Balance, T::Moment>;

    // On auction & bid: (auction_id, index) => bid_id
    AuctionBids get(auction_bids): map (T::Hash, u64) => T::Hash;
    AuctionBidsCount get(auction_bids_count): map T::Hash => u64 = 0;
    AuctionBidderBids get(auction_bidder_bids): map (T::Hash, T::AccountId) => T::Hash;

    Nonce: u64 = 0;
    // if you want to initialize value in storage, use genesis block
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
    NewBid(Hash, Balance),
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

      // TODO: ideally would be `end_time > now + AUCTION_MIN_DURATION`,
      //   but not sure how to do moment arithmetic.
      ensure!(end_time > now, "End time cannot be set less than 5 mins from current time");

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
        start_time: now,
        end_time: end_time.clone(),
        status: AuctionStatus::Ongoing,

        current_topmost_bids: Vec::new(),
        bid_price_to_topmost: <T::Balance as As<u64>>::sa(base_price.as_() - 1),
        displayed_topmost_bids: Vec::new(),

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
      //   1. only the auction_admin (which is kitty owner) can cancel the auction
      //   2. the now time is before auction end_time
      //   3. No one has bid in the auction yet

      // check #1:
      ensure!(<Auctions<T>>::exists(auction_id), "Auction does not exist");
      ensure!(Self::_auction_admin(auction_id) == sender, "You are not the auction admin");

      let auction = Self::auctions(auction_id);
      let now = <timestamp::Module<T>>::get();
      // check #2:
      ensure!(auction.end_time > now, "The auction has passed its end time");

      // check #3:
      ensure!(Self::auction_bids_count(auction_id) == 0, "Someone has bid already. So this auction cannot be cancelled");

      // write:
      //   1. update the auction status to cancelled.
      <Auctions<T>>::mutate(auction_id, |auction| auction.status = AuctionStatus::Cancelled);

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
      let mut auction = Self::auctions(auction_id);
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
      let bid = if <AuctionBidderBids<T>>::exists((auction_id, bidder.clone())) {
        let bid = Self::bids(Self::auction_bidder_bids((auction_id, bidder.clone())));
        // check the current bid is larger than its previous bid
        ensure!(bid_price > bid.price, "New bid has to be larger than your previous bid");
        <Bids<T>>::mutate(bid.id, |bid| bid.price = bid_price);
        bid
      } else {
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

        bid  // bid returned
      };

      // update auction bid info inside if higher than topmost
      if bid_price > auction.bid_price_to_topmost {
        auction.current_topmost_bids.push(bid.id);

        auction.current_topmost_bids.sort_by(|a, b| {
          let a_bp = Self::bids(a).price;
          let b_bp = Self::bids(b).price;
          a_bp.partial_cmp(&b_bp).unwrap()
        });
        auction.current_topmost_bids = auction.current_topmost_bids
          .into_iter().take(TOPMOST_BIDS_LEN).collect();

        // only set `bid_price_to_topmost` if the whole vector is filled
        if auction.current_topmost_bids.len() >= TOPMOST_BIDS_LEN {
          let bid = Self::bids(auction.current_topmost_bids[TOPMOST_BIDS_LEN - 1]);
          auction.bid_price_to_topmost = bid.price;
        }
      }

      // emit an event
      Self::deposit_event(RawEvent::NewBid(auction_id, bid_price));

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
    }

    // update corresponding storage
    <Kitties<T>>::insert(&kitty_id, kitty.clone());
    <KittiesArray<T>>::insert(Self::kitties_count(), &kitty_id);
    <KittiesCount<T>>::mutate(|cnt| *cnt += 1);

    // update OwnerKitties storage...
    if let Some(owner_id) = owner {
      <OwnerKitties<T>>::insert((owner_id.clone(), kitty.owner_pos.unwrap()), &kitty_id);
      <OwnerKittiesCount<T>>::mutate(owner_id, |cnt| *cnt += 1);
    }
    Ok(())
  }

  fn _add_auction_to_storage(auction: &Auction<T::Hash, T::Balance,
    T::Moment, AuctionTx<T::Hash, T::AccountId, T::Balance, T::Moment>>) -> Result
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
}
