Actor Function EN05MQ_GetPlayer()
    Actor player = None
    If CurrentPlayer != None
        player = CurrentPlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

Int Function EN05MQ_HistoricCommendations(Actor akPlayer)
    If akPlayer == None || TimesCompletedValues == None
        Return 0
    EndIf

    Int completed = 0
    Int index = 0
    While index < TimesCompletedValues.Length
        ActorValue completedValue = TimesCompletedValues[index]
        If completedValue != None
            Int value = akPlayer.GetValue(completedValue) as Int
            If value > 0
                completed += value
            EndIf
        EndIf
        index += 1
    EndWhile
    Return completed
EndFunction

Function EN05MQ_ReconcileCommendations(Actor akPlayer)
    If akPlayer == None
        Return
    EndIf

    If iCurrentCommendations < 0
        iCurrentCommendations = 0
    EndIf

    Int historic = EN05MQ_HistoricCommendations(akPlayer)
    If historic > iCurrentCommendations
        iCurrentCommendations = historic
    EndIf

    Int threshold = 0
    If EN05_MQ_CommendationThreshold != None
        threshold = EN05_MQ_CommendationThreshold.GetValue() as Int
    EndIf
    If threshold > 0 && iCurrentCommendations > threshold
        iCurrentCommendations = threshold
    EndIf

    If IsStageDone(iPlayerRegisteredStage) && !IsStageDone(iCommendationsCompletedStage)
        SetObjectiveDisplayed(10, True, True)
    EndIf
    If threshold > 0 && iCurrentCommendations >= threshold \
        && !IsStageDone(iCommendationsCompletedStage) && !IsStageDone(107)
        SetStage(107)
    EndIf
EndFunction

Function EN05MQ_AddCommendations(Int aiAmount)
    If aiAmount <= 0 || IsStageDone(iCommendationsCompletedStage)
        Return
    EndIf

    If iCurrentCommendations < 0
        iCurrentCommendations = 0
    EndIf
    iCurrentCommendations += aiAmount

    Int threshold = 0
    If EN05_MQ_CommendationThreshold != None
        threshold = EN05_MQ_CommendationThreshold.GetValue() as Int
    EndIf
    If threshold > 0 && iCurrentCommendations > threshold
        iCurrentCommendations = threshold
    EndIf

    If IsStageDone(iPlayerRegisteredStage)
        SetObjectiveDisplayed(10, True, True)
    EndIf
    If threshold > 0 && iCurrentCommendations >= threshold \
        && !IsStageDone(iCommendationsCompletedStage)
        SetStage(iCommendationsCompletedStage)
    EndIf
EndFunction

Event OnQuestInit()
    Actor player = EN05MQ_GetPlayer()
    If player == None
        Return
    EndIf

    Bool isPresident = False
    If EN06_EnclavePresidentFaction != None && player.IsInFaction(EN06_EnclavePresidentFaction)
        isPresident = True
    ElseIf EN06_Pres != None && EN06_Pres.IsCompleted()
        isPresident = True
    EndIf
    If isPresident
        If !IsStageDone(iCompletedPresidentalRace)
            SetStage(iCompletedPresidentalRace)
        EndIf
        Return
    EndIf

    Float checkpoint = fFirstTimeThroughValue
    If EN05_MQ_StageValue != None
        checkpoint = player.GetValue(EN05_MQ_StageValue)
    EndIf

    If checkpoint <= fFirstTimeThroughValue
        If !IsStageDone(iStartUpStage)
            SetStage(iStartUpStage)
        EndIf
    ElseIf checkpoint <= fDirectToRegisterValue
        If !IsStageDone(iPlayerHeardIntro)
            SetStage(iPlayerHeardIntro)
        EndIf
    Else
        EN05MQ_ReconcileCommendations(player)
        If !IsStageDone(iCommendationsCompletedStage) && !IsStageDone(iPlayerRegisteredStage)
            SetStage(iPlayerRegisteredStage)
        EndIf
    EndIf
EndEvent
