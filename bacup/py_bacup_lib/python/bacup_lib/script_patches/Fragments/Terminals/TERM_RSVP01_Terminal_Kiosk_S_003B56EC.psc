Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && RSVP00_AV_foundKesha
        playerRef.SetValue(RSVP00_AV_foundKesha, 1.0)
    EndIf
EndFunction

Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && RSVP00_AV_foundDelbert
        playerRef.SetValue(RSVP00_AV_foundDelbert, 1.0)
    EndIf
EndFunction
