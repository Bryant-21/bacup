Function EnsureHazard()
    If placedHazard == None && HazardToPlace != None
        placedHazard = PlaceAtMe(HazardToPlace)
    EndIf
EndFunction

Function ClearHazard()
    If placedHazard != None
        placedHazard.Disable()
        placedHazard.Delete()
        placedHazard = None
    EndIf
EndFunction

Event OnLoad()
    EnsureHazard()
EndEvent

Event OnUnload()
    ClearHazard()
EndEvent
