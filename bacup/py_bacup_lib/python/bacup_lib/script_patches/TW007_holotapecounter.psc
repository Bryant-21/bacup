; The FO76 counter is server-stripped. The quest fragment hands it the two clue holotape
; references it stocks at stage 300; playing either one sets its stage while the quest is
; still between PrereqRangeStart and PrereqRangeEnd.
Function WatchClueHolotape(ObjectReference akHolotapeRef)
    If akHolotapeRef
        RegisterForRemoteEvent(akHolotapeRef, "OnHolotapePlay")
    EndIf
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables
        questVariables.SetVariable("ClueReq", 2.0)
        questVariables.SetVariable("ClueCount", Count as Float)
    EndIf
EndFunction

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference aTerminalRef)
    Int currentStage = GetStage()
    If currentStage < PrereqRangeStart || currentStage > PrereqRangeEnd
        Return
    EndIf
    Form holotapeBase = akSender.GetBaseObject()
    Int stageToSet = -1
    If holotapeBase == SlideClue
        stageToSet = SlideStage
    ElseIf holotapeBase == MailClue
        stageToSet = MailStage
    EndIf
    If stageToSet < 0 || IsStageDone(stageToSet)
        Return
    EndIf
    Count += 1
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables
        questVariables.SetVariable("ClueCount", Count as Float)
    EndIf
    SetStage(stageToSet)
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
