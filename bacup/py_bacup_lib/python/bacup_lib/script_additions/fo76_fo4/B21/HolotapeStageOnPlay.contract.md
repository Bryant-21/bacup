# B21:HolotapeStageOnPlay

`B21:HolotapeStageOnPlay` is an ObjectReference listener for FO4's native `OnHolotapePlay` event. The FO76-to-FO4 converter attaches it only to `73BB19` (`A Lead at Last`) after verifying the mapped `73BAFC` Hells Eagles quest and the exact NOTE editor ID.

The listener sets stage 300 only while the mapped quest is running, stage 200 is done, and stage 300 is not done. It neither reacts to item pickup nor starts the quest. Stage 300 remains responsible for completing objective 20 and displaying objective 30.
