Function RedeemArcadePrize(Form akPrize, Int aiQuantity, Int aiPointCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && PointsAV != None && akPrize != None && playerRef.GetValue(PointsAV) >= aiPointCost
        playerRef.ModValue(PointsAV, -aiPointCost)
        playerRef.AddItem(akPrize, aiQuantity, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_PipePistol, 1, 200)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_38Ammo, 16, 100)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_PaddleBallString, 20, 100)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_BoiledWater, 1, 60)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemArcadePrize(Form_EmptyNukaColaBottle, 1, 20)
EndFunction
