Event OnQuestInit()
    EN05Intro_FillPlayer()
EndEvent

Function EN05Intro_FillPlayer()
    Actor player = Game.GetPlayer()
    If currentPlayer != None && currentPlayer.GetReference() == None && player != None
        currentPlayer.ForceRefTo(player)
    EndIf
EndFunction

Function EN05Intro_AdvanceTraining()
    If EN05_OfficerMisc_CompletedValue != None
        EN05_MQ_QuestScript officer = Game.GetFormFromFile(0x0010DBEA, "SeventySix.esm") as EN05_MQ_QuestScript
        If officer != None && officer.IsRunning() && !officer.IsCompleted() && !officer.IsStageDone(officer.iPlayerHeardIntro)
            officer.SetStage(officer.iPlayerHeardIntro)
        EndIf
    Else
        EN05_QuestScript basic = Game.GetFormFromFile(0x0008C87F, "SeventySix.esm") as EN05_QuestScript
        If basic != None && basic.IsRunning() && !basic.IsCompleted() && !basic.IsStageDone(basic.IntroStageComplete)
            basic.SetStage(basic.IntroStageComplete)
        EndIf
    EndIf
EndFunction

Event OnQuestShutdown()
    If SceneToShutdown != None && SceneToShutdown.IsPlaying()
        SceneToShutdown.Stop()
    EndIf
EndEvent
