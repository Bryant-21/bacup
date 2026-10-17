; This fragment's own PointsAV is unbound in the record VMAD; the sibling
; Nukacade_PrizeDispenserScript on the same TERM form carries the bound points AV.
Function RedeemArcadePrize(ObjectReference akTerminalRef, Form akPrize, Int aiQuantity, Int aiPointCost)
    Nukacade_PrizeDispenserScript dispenser = (Self as Form) as Nukacade_PrizeDispenserScript
    If dispenser != None
        dispenser.ClaimPrize(akTerminalRef, akPrize, aiQuantity, aiPointCost)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_BanditRoundupPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_BanditRoundupPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaZapperRacePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WhackACommiePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WeaponisedNukaColaPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WeaponisedNukaColaPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WhackACommiePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaZapperRacePlan, 1, 20000)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_BottleBlasterPlan, 1, 20000)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_BottleBlasterPlan, 1, 20000)
EndFunction
