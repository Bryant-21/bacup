; This fragment's own PointsAV is unbound in the record VMAD; the sibling
; Nukacade_PrizeDispenserScript on the same TERM form carries the bound points AV.
Function RedeemArcadePrize(ObjectReference akTerminalRef, Form akPrize, Int aiQuantity, Int aiPointCost)
    Nukacade_PrizeDispenserScript dispenser = (Self as Form) as Nukacade_PrizeDispenserScript
    If dispenser != None
        dispenser.ClaimPrize(akTerminalRef, akPrize, aiQuantity, aiPointCost)
    EndIf
EndFunction

Function Fragment_Terminal_00(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaColaQuantum, 1, 2000)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaWorldSpeakersPlan, 1, 12000)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukelelePlans, 1, 12000)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WeaponizedQuantum, 6, 12000)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_WeaponizedCherry, 6, 6000)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaQuantumGrenades, 1, 8000)
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaGrenades, 1, 5000)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_MiniNuke, 1, 2000)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_SuperStim, 1, 2000)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukaWorldSpeakersPlan, 1, 12000)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_NukelelePlans, 1, 12000)
EndFunction
