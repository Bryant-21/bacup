Function ENEvent_RecordCompletion()
    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    If CompletionTrackingValue != None
        player.SetValue(CompletionTrackingValue, player.GetValue(CompletionTrackingValue) + 1.0)
    EndIf

    If iCommendationValue <= 0 || EN05_MQ_Officer == None || !EN05_MQ_Officer.IsRunning()
        Return
    EndIf

    EN05_MQ_QuestScript officer = EN05_MQ_Officer as EN05_MQ_QuestScript
    If officer == None || !officer.IsStageDone(officer.iPlayerRegisteredStage) \
        || officer.IsStageDone(officer.iCommendationsCompletedStage)
        Return
    EndIf

    Int supportStage = officer.iCompletedBunkerPromotion
    If supportStage >= 0 && !officer.IsStageDone(supportStage)
        officer.SetObjectiveCompleted(supportStage)
        officer.SetStage(supportStage)
    EndIf

    officer.EN05MQ_AddCommendations(iCommendationValue)
EndFunction
