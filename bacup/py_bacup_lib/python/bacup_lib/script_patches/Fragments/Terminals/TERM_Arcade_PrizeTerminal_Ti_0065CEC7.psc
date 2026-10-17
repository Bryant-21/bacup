; This fragment's own PointsAV is unbound in the record VMAD; the sibling
; Nukacade_PrizeDispenserScript on the same TERM form carries the bound points AV.
Function RedeemArcadePrize(ObjectReference akTerminalRef, Form akPrize, Int aiQuantity, Int aiPointCost)
    Nukacade_PrizeDispenserScript dispenser = (Self as Form) as Nukacade_PrizeDispenserScript
    If dispenser != None
        dispenser.ClaimPrize(akTerminalRef, akPrize, aiQuantity, aiPointCost)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_ThirstZapper, 1, 6000)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NWOTJumpsuit, 1, 2500)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukacadeTokenDispenserPlan, 1, 800)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaColaWild, 1, 800)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_Missile, 1, 800)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WeaponizedNukaColaAmmo, 6, 1600)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_Stimpak, 1, 400)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_Psycho, 1, 400)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_45Ammo, 28, 600)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_Arrows, 16, 600)
EndFunction

Function Fragment_Terminal_12(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukacadeTokenDispenserPlan, 1, 800)
EndFunction
