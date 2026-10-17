; Called by the Arcade_PrizeTerminal tier fragments on the same TERM form. The claim flag is set
; before the transaction because the shared result submenu (644B0E) picks its body text from
; Nukacade_PrizeTerminalClaimChecker.validClaim on the terminal reference.
Bool Function ClaimPrize(ObjectReference akTerminalRef, Form akPrize, Int aiQuantity, Int aiPointCost)
    Actor playerRef = Game.GetPlayer()
    Bool canClaim = PointsAV != None && akPrize != None && aiQuantity > 0 && playerRef.GetValue(PointsAV) >= aiPointCost
    Nukacade_PrizeTerminalClaimChecker claimChecker = akTerminalRef as Nukacade_PrizeTerminalClaimChecker
    If claimChecker != None
        claimChecker.SetValidClaim(canClaim)
    EndIf
    If !canClaim
        If NotEnoughTicketsMsg != None
            NotEnoughTicketsMsg.Show()
        EndIf
        Return False
    EndIf
    playerRef.ModValue(PointsAV, -aiPointCost as Float)
    playerRef.AddItem(akPrize, aiQuantity)
    Return True
EndFunction
