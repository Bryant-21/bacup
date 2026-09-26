Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    If pBoS01 != None && pBoS01.IsRunning() && !pBoS01.IsStageDone(240) && !pBoS01.IsStageDone(210)
        pBoS01.SetStage(210)
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    Form password = Game.GetFormFromFile(0x003D0EF0, "SeventySix.esm")
    If playerRef == None || password == None || playerRef.GetItemCount(password) < 1 || akTerminalRef == None
        Return
    EndIf
    ObjectReference commandCenterDoor = akTerminalRef.GetLinkedRef()
    If commandCenterDoor != None
        commandCenterDoor.Lock(False)
    EndIf
EndFunction
