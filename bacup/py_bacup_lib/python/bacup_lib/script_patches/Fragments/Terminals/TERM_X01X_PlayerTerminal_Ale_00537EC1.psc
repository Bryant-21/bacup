Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    If Hopper_StartKeyword != None
        Hopper_StartKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
    EndIf
EndFunction
