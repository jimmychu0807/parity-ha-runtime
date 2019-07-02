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
  owner: AccountId,
  owner_pos: u64,
  in_auction: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum AuctionStatus {
  Ongoing,
  Cancelled,
  ToBeClaimed,
  Closed
}

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
  bid_count: u64,
  tx: Option<AuctionTx>,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug)]
pub enum BidStatus {
  Active,
  Withdrawn,
}

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

decl_event!(
  pub enum Event<T> where
    <T as system::Trait>::AccountId,
    <T as system::Trait>::Hash, {
    // <T as balances::Trait>::Balance, {

    // Events in our runtime
    KittyCreated(AccountId, Hash),
  }
);

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as CatAuction {
    Kitties get(kitties): map T::Hash => Kitty<T::Hash, T::AccountId>;
    AllKittiesArray get(kitty_array): map u64 => T::Hash;
    AllKittiesCount get(all_kitties_count): u64 = 0;

    // The following two go hand-in-hand, write to one likely need to update the other two
    OwnedKitties get(owned_kitty): map (T::AccountId, u64) => T::Hash;
    OwnedKittyCount get(owned_kitty_count): map T::AccountId => u64 = 0;

    // On Auction
    Auctions get(auctions): map T::Hash => Auction<T::Hash, T::Balance, T::Moment,
      AuctionTx<T::Hash, T::AccountId, T::Balance, T::Moment>>;
    AllAuctionsArray get(auction_array): map u64 => T::Hash;
    AllAuctionsCount get(all_auctions_count): u64 = 0;

    // On auction & bid: (auction_id, index) => bid_id
    AuctionBids: map (T::Hash, u64) => Option<T::Hash>;

    // `bid_id` => Bid object
    Bids get(bids): map T::Hash => Bid<T::Hash, T::AccountId, T::Balance, T::Moment>;

    Nonce: u64 = 0;
    // if you want to initialize value in storage, use genesis block
  }
}

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    fn deposit_event<T>() = default;

    // pub fn create_kitty(origin, kitty_name: Vec<u8>) -> Result {
    //   let sender = ensure_signed(origin)?;

    //   // generate a random hash key
    //   let nonce = <Nonce<T>>::get();
    //   let random_seed = <system::Module<T>>::random_seed();
    //   let kitty_id = (random_seed, &sender, nonce).using_encoded(<T as system::Trait>::Hashing::hash);

    //   // ensure the kitty_id is not existed
    //   ensure!(!<Kitties<T>>::exists(kitty_id), "Cat with the id existed already");

    //   let kitty = Kitty {
    //     id: kitty_id,
    //     name: Some(kitty_name),
    //     base_price: T::Balance::sa(0),
    //   };

    //   // add it in the storage
    //   <Kitties<T>>::insert(kitty_id, &kitty);
    //   <KittyOwner<T>>::insert(kitty_id, &sender);
    //   <OwnedKitties<T>>::mutate(&sender, |vec| vec.push(kitty_id));
    //   <AllKittiesCount<T>>::mutate(|cnt| *cnt += 1);

    //   // nonce increment by 1
    //   <Nonce<T>>::mutate(|nonce| *nonce += 1);

    //   // emit an event
    //   Self::deposit_event(RawEvent::Created(sender, kitty_id));

    //   Ok(())
    // } // end of fn `create_kitty`

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
