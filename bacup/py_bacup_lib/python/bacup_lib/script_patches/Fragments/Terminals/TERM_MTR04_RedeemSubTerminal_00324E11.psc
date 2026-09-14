Function RedeemPrize(Form akPrize, Int aiQuantity, Int aiTokenCost)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Token != None && akPrize != None && playerRef.GetItemCount(Token) >= aiTokenCost
        playerRef.RemoveItem(Token, aiTokenCost, True)
        playerRef.AddItem(akPrize, aiQuantity, False)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    RedeemPrize(PencilTop, 1, 5)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RedeemPrize(MiningHelmet, 1, 20)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    RedeemPrize(CommieWhacker, 1, 50)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    RedeemPrize(MascotSuit, 1, 150)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    RedeemPrize(MascotHead, 1, 300)
EndFunction

Function Fragment_Terminal_08(ObjectReference akTerminalRef)
    RedeemPrize(CottonCandy, 1, 5)
EndFunction

Function Fragment_Terminal_09(ObjectReference akTerminalRef)
    RedeemPrize(Gumdrops, 1, 5)
EndFunction

Function Fragment_Terminal_10(ObjectReference akTerminalRef)
    RedeemPrize(PaddleBallAmmo, 10, 5)
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    RedeemPrize(PaddleBall, 1, 50)
EndFunction

Function Fragment_Terminal_12(ObjectReference akTerminalRef)
    RedeemPrize(Comic, 1, 100)
EndFunction
