Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && AV_MrFluffy != None
        playerRef.SetValue(AV_MrFluffy, 1.0)
    EndIf
EndFunction
