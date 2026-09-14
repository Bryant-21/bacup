Function RedeemArcadePrize(Form akPrize, Int aiQuantity, Int aiPointCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && PointsAV != None && akPrize != None && playerRef.GetValue(PointsAV) >= aiPointCost
        playerRef.ModValue(PointsAV, -aiPointCost)
        playerRef.AddItem(akPrize, aiQuantity, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_ThirstZapper, 1, 6000)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NWOTJumpsuit, 1, 2500)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukacadeTokenDispenserPlan, 1, 800)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukaColaWild, 1, 800)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Missile, 1, 800)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_WeaponizedNukaColaAmmo, 6, 1600)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Stimpak, 1, 400)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Psycho, 1, 400)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_45Ammo, 28, 600)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_Arrows, 16, 600)
EndFunction

Function Fragment_Terminal_12(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_NukacadeTokenDispenserPlan, 1, 800)
EndFunction
