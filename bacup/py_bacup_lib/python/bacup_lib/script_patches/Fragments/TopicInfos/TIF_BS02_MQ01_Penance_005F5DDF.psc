Function Fragment_End(ObjectReference akSpeakerRef)
    If Alias_Actor_Hewsen_Caverns != None && Alias_Marker_HewsenDeadEnd != None
        ObjectReference hewsen = Alias_Actor_Hewsen_Caverns.GetReference()
        ObjectReference hewsenMarker = Alias_Marker_HewsenDeadEnd.GetReference()
        If hewsen != None && hewsenMarker != None
            hewsen.MoveTo(hewsenMarker)
        EndIf
    EndIf
    If Alias_Actor_Norland_Caverns != None && Alias_Marker_NorlandDeadEnd != None
        ObjectReference norland = Alias_Actor_Norland_Caverns.GetReference()
        ObjectReference norlandMarker = Alias_Marker_NorlandDeadEnd.GetReference()
        If norland != None && norlandMarker != None
            norland.MoveTo(norlandMarker)
        EndIf
    EndIf
EndFunction
