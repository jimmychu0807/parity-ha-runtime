/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references

/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

use support::{decl_module, decl_storage, decl_event, dispatch::Result,
  StorageValue, StorageMap, ensure, traits::Currency };
use { system::ensure_signed, timestamp };

// this is needed when you want to use Vec and Box
use rstd::prelude::*;
use runtime_io;
use runtime_primitives::traits::{ As, CheckedAdd, CheckedDiv, CheckedMul, Hash };
use parity_codec::{ Encode, Decode };

pub type StdResult<T> = rstd::result::Result<T, &'static str>;

/// The module's configuration trait. This is trait inheritance.
pub trait Trait: timestamp::Trait + balances::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
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
      ensure!(end_time > now, "End time cannot be set before current time");

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

      // write:
      //   1. update the auction status to cancelled.
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
}
