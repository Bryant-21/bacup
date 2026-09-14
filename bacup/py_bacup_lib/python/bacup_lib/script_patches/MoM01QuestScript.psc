Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != CONST_MoM01_MeetYourMentor
        Return
    EndIf

    ObjectReference mentorCorpse = MistressCorpse.GetReference()
    If mentorCorpse != None
        ObjectReference mentorHolotape = MoM01MentorHolotape.GetReference()
        If mentorHolotape != None && mentorHolotape.GetContainer() != mentorCorpse
            mentorCorpse.AddItem(mentorHolotape, 1, True)
        EndIf
        ObjectReference mentorLogin = MoM01MentorLogin.GetReference()
        If mentorLogin != None && mentorLogin.GetContainer() != mentorCorpse
            mentorCorpse.AddItem(mentorLogin, 1, True)
        EndIf
    EndIf

    ObjectReference raiderCorpse = RaiderCorpse.GetReference()
    ObjectReference raiderNote = MoM01RaiderNote.GetReference()
    If raiderCorpse != None && raiderNote != None && raiderNote.GetContainer() != raiderCorpse
        raiderCorpse.AddItem(raiderNote, 1, True)
    EndIf
EndEvent
