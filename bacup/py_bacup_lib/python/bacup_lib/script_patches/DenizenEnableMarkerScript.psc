Event OnLoad()
    ObjectReference markerToEnable = GetLinkedRef(DenizenLinkToEnableMarker)
    If markerToEnable != None
        markerToEnable.Enable()
    EndIf
EndEvent
