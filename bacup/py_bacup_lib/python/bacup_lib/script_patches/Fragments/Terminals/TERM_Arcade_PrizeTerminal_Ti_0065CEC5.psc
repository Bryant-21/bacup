; This fragment's own PointsAV is unbound in the record VMAD; the sibling
; Nukacade_PrizeDispenserScript on the same TERM form carries the bound points AV.
Function RedeemArcadePrize(ObjectReference akTerminalRef, Form akPrize, Int aiQuantity, Int aiPointCost)
    Nukacade_PrizeDispenserScript dispenser = (Self as Form) as Nukacade_PrizeDispenserScript
    If dispenser != None
        dispenser.ClaimPrize(akTerminalRef, akPrize, aiQuantity, aiPointCost)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_PipePistol, 1, 200)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_38Ammo, 16, 100)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_PaddleBallString, 20, 100)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_BoiledWater, 1, 60)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(akTerminalRef, Form_EmptyNukaColaBottle, 1, 20)
EndFunction
