Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    If Corpse_Startkeyword != None
        Corpse_Startkeyword.SendStoryEventAndWait(None, Game.GetPlayer())
    EndIf
EndFunction
