On Runtime:

  * Seller can list his kitty to be in auction with a base price, and specify the auction period (with the minimum auction period specified in runtime).
  * Seller can change his mind and cancel the auction if no one has bidden on the kitty
  * Multiple bidders can come to bid for the kitty. Their bidding price are not revealed, but the ranking of top 3 bidders are evaluated every period (period are specified in runtime, say 10 mins) and revealed.
  * When a bidder bids, the bidding amount is being put in escrow.
  * All bidders except the top 3 bidders in the last period can retreat from the bid, and get their bidding amount back
  * Bidders can always update their bidding amount, but the bidding ranking is not updated until end of every period.
  * Finally, the winning bidder pay the second highest value to purchase the kitty from the seller. Remaining bidding amount are returned to all bidders.
  * some manual test cases and automated test cases

On UI:

  * Bidding Page: a page showing all kitties under auction in the universe.
  * Each kitty shows the kitty owner, kitty base price, current number of bidders involved, and the ranking of the top 3 bidders.
  * A player can place bid, or update his bid.
  * Result Page: a page showing the history of when a kitty was sold from which player to which player at what price.

Build a small server/listener:

  * Listen to the event from the chain and save as off-chain cache in mongodb.
  * a cron job to call the substrate node regularly to update display bids.
