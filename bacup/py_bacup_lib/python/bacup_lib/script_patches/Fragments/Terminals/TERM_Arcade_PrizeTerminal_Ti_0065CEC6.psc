Function RedeemArcadePrize(Form akPrize, Int aiQuantity, Int aiPointCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && PointsAV != None && akPrize != None && playerRef.GetValue(PointsAV) >= aiPointCost
        playerRef.ModValue(PointsAV, -aiPointCost)
        playerRef.AddItem(akPrize, aiQuantity, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_10mmAmmo, 28, 200)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_GoldfishPlan, 1, 1600)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NWOTShirt, 1, 1600)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_CommieWhacker, 1, 800)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukaCherry, 1, 600)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Snowglobe, 1, 1600)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Snowglobe, 1, 1600)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_CommieWhacker, 1, 800)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BaseballGrenade, 1, 200)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_GoldfishPlan, 1, 1600)
EndFunction
