Function Fragment_End(ObjectReference akSpeakerRef)
    If Game.GetPlayer() != None
        Game.GetPlayer().SetValue(pNWOT_FortuneTeller_AboutOtherPeopleAV, 1.0)
    EndIf
EndFunction
