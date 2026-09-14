Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && EN06_OptOutofNotifications
        playerRef.SetValue(EN06_OptOutofNotifications, 1.0)
    EndIf
EndFunction

Function Fragment_Terminal_07(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && EN06_OptOutofNotifications
        playerRef.SetValue(EN06_OptOutofNotifications, 0.0)
    EndIf
EndFunction
