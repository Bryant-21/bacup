Function RedeemClaimTokens(LeveledItem akReward, Int aiTokenCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && MTR08_ClaimToken != None && akReward != None && playerRef.GetItemCount(MTR08_ClaimToken) >= aiTokenCost
        playerRef.RemoveItem(MTR08_ClaimToken, aiTokenCost, True)
        playerRef.AddItem(akReward, 1, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemClaimTokens(MTR08_LL_01_MineHaulStandard, 10)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    RedeemClaimTokens(MTR08_LL_03_MineHaulGreat, 40)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemClaimTokens(MTR08_LL_02_MineHaulGood, 20)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemClaimTokens(MTR08_LL_04_MineHaulJackpot, 100)
EndFunction
