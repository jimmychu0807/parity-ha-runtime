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
    AllKittiesArray get(kitty_array): map u64 => T::Hash;
    AllKittiesCount get(all_kitties_count): u64 = 0;

    // The following two go hand-in-hand, write to one likely need to update the other two
    OwnerKitties get(owner_kitties): map (T::AccountId, u64) => T::Hash;
    OwnerKittiesCount get(owner_kitties_count): map T::AccountId => u64 = 0;

    // On Auction
    Auctions get(auctions): map T::Hash => Auction<T::Hash, T::Balance, T::Moment,
      AuctionTx<T::Hash, T::AccountId, T::Balance, T::Moment>>;
    AllAuctionsArray get(auction_array): map u64 => T::Hash;
    AllAuctionsCount get(all_auctions_count): u64 = 0;

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
    <T as system::Trait>::Hash, {
    // <T as balances::Trait>::Balance, {

    // Events in our runtime
    KittyCreated(AccountId, Hash, Vec<u8>),
  }
);

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    fn deposit_event<T>() = default;

    pub fn create_kitty(origin, kitty_name: Vec<u8>) -> Result {
      let sender = ensure_signed(origin)?;

      // generate a random hash key
      let nonce = <Nonce<T>>::get();
      let random_seed = <system::Module<T>>::random_seed();
      let kitty_id = (random_seed, &sender, nonce).using_encoded(<T as system::Trait>::Hashing::hash);

      // ensure the kitty_id is not existed
      ensure!(!<Kitties<T>>::exists(kitty_id), "Cat with the id existed already");

      let owner_kitties_count = Self::owner_kitties_count(&sender);

      let kitty = Kitty {
        id: kitty_id,
        name: Some(kitty_name.clone()),
        owner: Some(sender.clone()),
        owner_pos: Some(owner_kitties_count),
        in_auction: false,
      };

      // update corresponding storage
      <Kitties<T>>::insert(kitty_id, &kitty);
      <AllKittiesArray<T>>::insert(Self::all_kitties_count(), &kitty_id);
      <AllKittiesCount<T>>::mutate(|cnt| *cnt += 1);

      // update OwnerKitties...
      <OwnerKitties<T>>::insert((sender.clone(), owner_kitties_count), &kitty_id);
      <OwnerKittiesCount<T>>::mutate(&sender, |cnt| *cnt += 1);

      // nonce increment by 1
      <Nonce<T>>::mutate(|nonce| *nonce += 1);

      // emit an event
      Self::deposit_event(RawEvent::KittyCreated(sender, kitty_id, kitty_name));

      Ok(())
    } // end of fn `create_kitty`

    pub fn start_auction(origin, kitty_id: T::Hash, end_time: T::Moment,
      base_price: T::Balance) -> Result {

      let sender = ensure_signed(origin)?;
      // Check:
      //  1. ensure kitty exists, and the kitty.owner == sender
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
      //  2. set the kitty state in_auction = true


      Ok(())
    }

    // pub fn for_sale(origin, kitty_id: T::Hash, base_price: u64) -> Result {
    //   let sender = ensure_signed(origin)?;

    //   // 1. check the origin own the kitty
    //   // 2. check the price is > 0
    //   let kitty_owner = Self::owner_of(kitty_id).ok_or("Kitty has no owner")?;
    //   ensure!(kitty_owner == sender, "The cat is not owned by the requester.");
    //   ensure!(base_price > 0, "The price must be set higher than 0.");

    //   // 3. set the base price of the kitty, write to blockchain
    //   <Kitties<T>>::mutate(kitty_id, |kitty| {
    //     kitty.base_price = T::Balance::sa(base_price);
    //   });

    //   // 4. emit an event
    //   Self::deposit_event(RawEvent::ForSale(kitty_id, base_price));

    //   // 5. return
    //   Ok(())
    // }

    // pub fn cancel_sale(origin, kitty_id: T::Hash) -> Result {
    //   let sender = ensure_signed(origin)?;

    //   // 1. check the origin own the kitty
    //   // 2. check the price is > 0
    //   let kitty_owner = Self::owner_of(kitty_id).ok_or("Kitty has no owner")?;
    //   ensure!(kitty_owner == sender, "The cat is not owned by the requester.");

    //   // 3. set the base price of the kitty to 0
    //   <Kitties<T>>::mutate(kitty_id, |kitty| {
    //     kitty.base_price = T::Balance::sa(0);
    //   });

    //   Ok(())
    // }

    // pub fn transaction(origin, kitty_id: T::Hash) -> Result {
    //   let sender = ensure_signed(origin)?;

    //   // 1. get the kitty
    //   //   - if the kitty has no owner, throw error
    //   //   - check the user at least kitty.base_price in his balance
    //   let kitty_owner = Self::owner_of(kitty_id).ok_or("Kitty has no owner.")?;
    //   let mut kitty = Self::kitties(kitty_id);
    //   let transaction_price = kitty.base_price;

    //   ensure!(transaction_price.as_() > 0, "This kitty is not for sale.");
    //   ensure!(sender != kitty_owner, "You cannot purchase from your own");
    //   ensure!(<balances::Module<T>>::free_balance(&sender) >= transaction_price, "You don't have enough balance to purchase this kitty.");

    //   // 2. do the exchange
    //   //   - sender balance is deducted, kitty owner balance is incremented
    //   <balances::Module<T> as Currency<_>>::transfer(&sender, &kitty_owner, transaction_price)?;

    //   //   - the kitty is changed hand, update all relevant data structure
    //   // a. update kitty with `base_price` set to 0,
    //   kitty.base_price = T::Balance::sa(0);
    //   <Kitties<T>>::insert(kitty.id, kitty);

    //   // b. update kitty owner storage
    //   <KittyOwner<T>>::insert(kitty.id, &sender);

    //   // c. update ownedKitties storage,
    //   //   c1. remove from original owner
    //   <OwnedKitties<T>>::mutate(kitty_owner, |owned_vec| {
    //     // ENHANCE: find a better way of writing this:
    //     let kitty_index = 0;
    //     for (i, el) in owned_vec.iter().enumerate() {
    //       if el.as_bytes() == kitty.id.as_bytes() {
    //         kitty_index = i;
    //         break;
    //       }
    //     }
    //     owned_vec.remove(kitty_index);
    //   });

    //   //   c2. add to the new owner
    //   <OwnedKitties<T>>::mutate(sender, |owned_vec| {
    //     owned_vec.push(kitty.id)
    //   });

    //   // 3. emit an event
    //   Self::deposit_event(RawEvent::Transaction(kitty_owner, sender, kitty_id, transaction_price.as_()));

    //   // 4. return
    //   Ok(())
    // } // end of `transaction`
  }
}
