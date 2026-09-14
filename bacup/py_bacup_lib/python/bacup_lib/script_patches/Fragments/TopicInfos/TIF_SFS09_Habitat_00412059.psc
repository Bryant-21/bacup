Function Fragment_Begin(ObjectReference akSpeakerRef)
    If SFS09_Habitat_Misc_StartKeyword != None
        SFS09_Habitat_Misc_StartKeyword.SendStoryEvent(akRef1 = Game.GetPlayer(), akRef2 = akSpeakerRef)
    EndIf
EndFunction
