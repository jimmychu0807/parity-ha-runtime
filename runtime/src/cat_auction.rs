/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

use support::{decl_module, decl_storage, decl_event, dispatch::Result,
  StorageValue, StorageMap, ensure };
use system::ensure_signed;

// this is needed when you want to use Vec and Box
use rstd::prelude::*;
use runtime_primitives::traits::{ As, Hash };
use parity_codec::{ Encode, Decode };

// question: why I cannot use std::fmt ?
// use std::{ fmt };


// Our own Cat struct
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Kitty<Hash, Balance> {
  id: Hash,
  name: Option<Vec<u8>>,
  base_price: Balance, // when 0, it is not for sale
}

/// The module's configuration trait.
pub trait Trait: balances::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_event!(
  pub enum Event<T> where
    <T as system::Trait>::AccountId,
    <T as system::Trait>::Hash, {
    // <T as balances::Trait>::Balance, {

    // Events in our runtime
    Created(AccountId, Hash),
    ForSale(Hash, u64),

    // FromAccountId, ToAccountId, KittyId
    Transaction(AccountId, AccountId, Hash, u64),
  }
);

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as CatAuction {
    Kitties get(kitties): map T::Hash => Kitty<T::Hash, T::Balance>;
    KittyOwner get(owner_of): map T::Hash => Option<T::AccountId>;
    OwnedKitties get(kitties_owned): map T::AccountId => Vec<T::Hash> = Vec::new();

    AllKittiesCount get(all_kitties_cnt): u64;
    Nonce: u64;

    // if you want to initialize value in storage, use genesis block
  }
}

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

      let kitty = Kitty {
        id: kitty_id,
        name: Some(kitty_name),
        base_price: <T::Balance as As<u64>>::sa(0),
      };

      // add it in the storage
      <Kitties<T>>::insert(kitty_id, &kitty);
      <KittyOwner<T>>::insert(kitty_id, &sender);
      <OwnedKitties<T>>::mutate(&sender, |vec| vec.push(kitty_id));
      <AllKittiesCount<T>>::mutate(|cnt| *cnt += 1);

      // nonce increment by 1
      <Nonce<T>>::mutate(|nonce| *nonce += 1);

      // generate an event
      Self::deposit_event(RawEvent::Created(sender, kitty_id));

      Ok(())
    } // end of fn `create_kitty`

    pub fn for_sale(origin, kitty_id: T::Hash, base_price: u64) -> Result {
      let sender = ensure_signed(origin)?;

      // 1. check the origin own the kitty
      // 2. check the price is > 0
      let kitty_owner = Self::owner_of(kitty_id).ok_or("Kitty has no owner")?;
      ensure!(kitty_owner == sender, "The cat is not owned by the requester.");
      ensure!(base_price > 0, "The price must be set higher than 0.");

      // 3. set the base price of the kitty, write to blockchain
      <Kitties<T>>::mutate(kitty_id, |kitty| {
        kitty.base_price = <T::Balance as As<u64>>::sa(base_price);
      });

      // 4. announce with an event.
      Self::deposit_event(RawEvent::ForSale(kitty_id, base_price));

      // 5. return
      Ok(())
    }

    pub fn cancel_sale(origin, kitty_id: T::Hash) -> Result {
      let sender = ensure_signed(origin)?;

      // 1. check the origin own the kitty
      // 2. check the price is > 0
      let kitty_owner = Self::owner_of(kitty_id).ok_or("Kitty has no owner")?;
      ensure!(kitty_owner == sender, "The cat is not owned by the requester.");

      // 3. set the base price of the kitty to 0
      <Kitties<T>>::mutate(kitty_id, |kitty| {
        kitty.base_price = <T::Balance as As<u64>>::sa(0);
      });

      Ok(())
    }

  }
}
