Function SetPlayerTerminalValue(ActorValue akValue)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && akValue != None
        playerRef.SetValue(akValue, 1.0)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(TylerCountyStatus)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(PointPleasantStatus)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(FlatwoodsStatus)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(HelvetiaStatus)
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(SummersvilleStatus)
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    SetPlayerTerminalValue(MorgantownStatus)
EndFunction
