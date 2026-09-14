Function RedeemArcadePrize(Form akPrize, Int aiQuantity, Int aiPointCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && PointsAV != None && akPrize != None && playerRef.GetValue(PointsAV) >= aiPointCost
        playerRef.ModValue(PointsAV, -aiPointCost)
        playerRef.AddItem(akPrize, aiQuantity, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BanditRoundupPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BanditRoundupPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukaZapperRacePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_WhackACommiePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_WeaponisedNukaColaPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_WeaponisedNukaColaPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_WhackACommiePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukaZapperRacePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BottleBlasterPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BottleBlasterPlan, 1, 20000)
EndFunction
