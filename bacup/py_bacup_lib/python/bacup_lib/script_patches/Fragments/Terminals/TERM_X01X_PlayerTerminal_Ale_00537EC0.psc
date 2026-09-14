Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If LGVendor != None
        LGVendor.SendStoryEventAndWait(None, Game.GetPlayer())
    EndIf
EndFunction
