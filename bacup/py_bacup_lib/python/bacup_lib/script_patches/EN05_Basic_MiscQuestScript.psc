Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If CurrentPlayer != None && CurrentPlayer.GetReference() == None && player != None
        CurrentPlayer.ForceRefTo(player)
    EndIf
    Quest basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as Quest
    If basic != None
        RegisterForRemoteEvent(basic, "OnStageSet")
        EN05Misc_ReconcileTraining(basic)
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == Game.GetFormFromFile(0x0008C87F, "SeventySix.esm")
        EN05Misc_ReconcileTraining(akSender)
    EndIf
EndEvent

Function EN05Misc_ReconcileTraining(Quest akBasic)
    If !IsRunning() || IsStageDone(iCompletionStage) || akBasic == None
        Return
    EndIf
    If akBasic.IsStageDone(10) || akBasic.IsStageDone(20) || akBasic.IsCompleted()
        SetStage(iCompletionStage)
    EndIf
EndFunction
