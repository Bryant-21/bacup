Function Fragment_End(ObjectReference akSpeakerRef)
    If MorgantownMapMarker != None
        MorgantownMapMarker.AddToMap(False)
    EndIf
    If MorgantownStationMapMarker != None
        MorgantownStationMapMarker.AddToMap(False)
    EndIf
    If MorgantownTrainyardMapMarker != None
        MorgantownTrainyardMapMarker.AddToMap(False)
    EndIf
EndFunction
