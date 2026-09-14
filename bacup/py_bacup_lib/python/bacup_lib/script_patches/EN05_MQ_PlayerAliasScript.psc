Bool Function EN05MQ_AlreadyCounted(Actor akVictim)
    If KilledEpics == None
        Return False
    EndIf
    Return KilledEpics.Find(akVictim) >= 0
EndFunction

Function EN05MQ_RememberKill(Actor akVictim)
    If KilledEpics == None
        KilledEpics = new Actor[1]
        KilledEpics[0] = akVictim
    Else
        KilledEpics.Add(akVictim)
    EndIf
    StartTimer(iKilledListTimerLength as Float, iClearKilledListTimerID)
EndFunction

Int Function EN05MQ_CommendationValueFor(Actor akVictim)
    If EpicRankAV != None && EN05_Officer_EpicAVThreshold != None \
        && akVictim.GetValue(EpicRankAV) >= EN05_Officer_EpicAVThreshold.GetValue()
        If EN05_Officer_Legendary_KillCommendationValue != None
            Return EN05_Officer_Legendary_KillCommendationValue.GetValue() as Int
        EndIf
        Return 0
    EndIf

    Bool isTarget = False
    If ScorchBeastRace != None && akVictim.GetRace() == ScorchBeastRace
        isTarget = True
    ElseIf CB15_QuestKillTarget != None && akVictim.HasKeyword(CB15_QuestKillTarget)
        isTarget = True
    EndIf

    If isTarget && EN05_MQ_KillCommendenationValue != None
        Return EN05_MQ_KillCommendenationValue.GetValue() as Int
    EndIf
    Return 0
EndFunction

Event OnKill(Actor akVictim)
    If akVictim == None
        Return
    EndIf

    Quest owner = GetOwningQuest()
    EN05_MQ_QuestScript officer = owner as EN05_MQ_QuestScript
    If officer == None || !owner.IsRunning()
        Return
    EndIf
    If !owner.GetStageDone(iCommendationBegunStage) || owner.GetStageDone(iCommendationCompletionStage)
        Return
    EndIf
    If EN05MQ_AlreadyCounted(akVictim)
        Return
    EndIf

    Int awarded = EN05MQ_CommendationValueFor(akVictim)
    If awarded <= 0
        Return
    EndIf
    EN05MQ_RememberKill(akVictim)

    Actor player = GetActorReference()
    If player == None
        player = Game.GetPlayer()
    EndIf
    If player != None && EN05_Officer_KillCompletedValue != None
        player.SetValue(EN05_Officer_KillCompletedValue, player.GetValue(EN05_Officer_KillCompletedValue) + 1.0)
    EndIf

    Int legendaryValue = 0
    If EN05_Officer_Legendary_KillCommendationValue != None
        legendaryValue = EN05_Officer_Legendary_KillCommendationValue.GetValue() as Int
    EndIf
    If legendaryValue > 0 && awarded >= legendaryValue && !owner.GetStageDone(iFirstLegendaryKill)
        owner.SetObjectiveCompleted(iFirstLegendaryKill)
        owner.SetStage(iFirstLegendaryKill)
    EndIf

    officer.EN05MQ_AddCommendations(awarded)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iClearKilledListTimerID
        KilledEpics = None
    EndIf
EndEvent
