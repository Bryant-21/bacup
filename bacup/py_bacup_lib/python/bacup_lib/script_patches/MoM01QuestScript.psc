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

    ObjectReference raiderCorpseRef = RaiderCorpse.GetReference()
    ObjectReference raiderNote = MoM01RaiderNote.GetReference()
    If raiderCorpseRef != None && raiderNote != None && raiderNote.GetContainer() != raiderCorpseRef
        raiderCorpseRef.AddItem(raiderNote, 1, True)
    EndIf
EndEvent
Function ReconcileTerminalState()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MoM01TerminalValue == None
        Return
    EndIf
    If IsStageDone(80)
        playerRef.SetValue(MoM01TerminalValue, CONST_MoM01Value_ReadyForReport)
    ElseIf IsStageDone(50)
        playerRef.SetValue(MoM01TerminalValue, CONST_MoM01Value_RequestedMentor)
    ElseIf IsStageDone(40)
        playerRef.SetValue(MoM01TerminalValue, CONST_MoM01Value_ReadyForMentor)
    EndIf
EndFunction
