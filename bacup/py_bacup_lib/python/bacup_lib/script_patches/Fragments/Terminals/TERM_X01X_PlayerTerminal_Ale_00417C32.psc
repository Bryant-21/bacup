Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    If Arktos_Startkeyword != None
        Arktos_Startkeyword.SendStoryEventAndWait(None, Game.GetPlayer())
    EndIf
EndFunction
