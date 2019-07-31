# Parity HA Runtime

- [Video demo](https://youtu.be/Ru7_BeX1a1g)
- [Substrate Runtime code repository](https://github.com/jimmychu0807/parity-ha-runtime)
- [React frontend code repository](https://github.com/jimmychu0807/parity-ha-ui)

### Overall

This is the substrate runtime of kitty auction, inspired from the [substrate collectibles tutorial](https://substrate.dev/substrate-collectables-workshop/).

This runtime implements the following features:

  - *Creating a new kitty* - taking parameters of: 1) kitty name  
    A new kitty is created.

  - *Creating a new auction* - taking parameters of: 1) kitty ID, 2) kitty base price, 3) auction end time  
    A new auction is created.

  - *Cancelling an auction*  
    When no one has placed a bid yet the auction is cancelled.

  - *Bidding in an ongoing auction* - taking parameters of: 1) auction ID, 2) bidding price.    
    Either a new bid is placed in the auction, or a bid has increased his previous bidding offer. When a bid is successfully placed, the money of the bidder is held in reserve.

  - *Closing an auction*  
    When the auction ending time is reached, anyone can call this function to close the auction. If conditions are met, the kitty is transferred to the bidder and money from the winner transferred to the original kitty owner. Bids from other bidders are returned.

There are features planned during the design phase but not really implemented/tested:

  - The current bidding ranking of an auction is not known to the public. The bidding ranking is only updated regularly via function `update_auction_display_bids` being called from another service.

  - Logic of an auction winner paying the second highest bid.

Original design requirements can be seen [here](docs/requirements.md).

### Implementation notes

  - Within the Kitty object, there are `owner`, and `owner_pos` attributes. With hindsight, I think this is not a good design. Everytime when a kitty is transferred, I now need to update the Kitty object also.

  - To me, what `decl_storage!` is to the runtime is like what a database to a backend. When we need an index to lookup for an object or a new relation between objects, we need a storage item. The more relations we have, the more storage items we need. It soon becomes a hassle (and error-prone) to keep track of what need to be updated when we want to update these relations.

### Deployment notes

  - The substrate runtime is deployed in an external server run with `--dev` flag, so testing accounts exist.
  - Nginx is configured as proxy to take external `wss` connections and forward them to substrate socket that listen to localhost with `ws` protocol.

### Testing notes

  - A manual testing scenario is written [here](https://github.com/jimmychu0807/parity-ha-runtime/issues/1).
  - Test cases are written on:
    - a kitty can be created,
    - auction can be created, accept bid, and allowed closed, with transaction occured.

### Further Enhancement

  - Refine the object design
  - Kitty name be stored in external decentralized storage (IPFS?)
  - Kitty has an image that can be uploaded by user and stored in external decentralized storage (IPFS?)
  - Allow user to create an account and login
