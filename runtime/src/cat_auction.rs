/// A runtime module template with necessary imports

/// Feel free to remove or edit this file as needed.
/// If you change the name of this file, make sure to update its references in runtime/src/lib.rs
/// If you remove this file, you can remove those references


/// For more guidance on Substrate modules, see the example module
/// https://github.com/paritytech/substrate/blob/master/srml/example/src/lib.rs

use support::{decl_module, decl_storage, decl_event, dispatch::Result,
  StorageValue, StorageMap };
use system::ensure_signed;
use parity_codec::{ Encode, Decode };

// Our own Cat struct
#[derive(Encode, Decode, Default, Clone, PartialEq, Debug)]
pub struct Kitty<Hash, Balance> {
  id: Hash,
  price: Balance,
}

/// The module's configuration trait.
pub trait Trait: balances::Trait {
  type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_event!(
  pub enum Event<T> where
    <T as system::Trait>::AccountId,
    <T as system::Trait>::Hash {

    // Events in our runtime
    Created(AccountId, Hash),
  }
);

// This module's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as CatAuctionModule {
    Kitties get(kitties): map T::Hash => Kitty<T::Hash, T::Balance>;
    KittyOwner get(owner_of): map T::Hash => Option<T::AccountId>;

    Nonce: u64;
  }
}

decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {
    fn deposit_event<T>() = default;

    pub fn create_kitty(origin) -> Result {
      let sender = ensure_signed(origin)?;

      // generate a random hash key
      let nonce = <Nonce<T>>::get();



      Ok(())
    }
    // end of fn `create_kitty`
  }
}
