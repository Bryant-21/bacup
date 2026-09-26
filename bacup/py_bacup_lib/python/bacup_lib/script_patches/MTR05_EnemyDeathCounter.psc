Event OnAliasInit()
    BeginTracking()
EndEvent

Event OnAliasReset()
    BeginTracking()
EndEvent

Event OnAliasShutdown()
    CancelTimer(6205)
    bRegistrationComplete = False
EndEvent

Function BeginTracking()
    CancelTimer(6205)
    bRegistrationComplete = True
    StartTimer(2.0, 6205)
EndFunction

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    EvaluateEnemiesRemaining()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 6205
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.IsStopped() || owningQuest.GetStageDone(iShutdownStage)
        bRegistrationComplete = False
        Return
    EndIf
    EvaluateEnemiesRemaining()
    If bRegistrationComplete
        StartTimer(2.0, 6205)
    EndIf
EndEvent

Int Function CountLivingEnemies()
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor enemyActor = GetAt(index) as Actor
        If enemyActor != None && !enemyActor.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Bool Function WavesStillSpawning(Quest akOwningQuest)
    DefaultQuestEncounterWaveScript waveScript = akOwningQuest as DefaultQuestEncounterWaveScript
    If waveScript == None
        Return False
    EndIf
    ; Waves one and two are the only live rows; three and four ship as OBSOLETE.
    Return waveScript.IsEncounterWaveSpawning(0) || waveScript.IsEncounterWaveSpawning(1)
EndFunction

Function EvaluateEnemiesRemaining()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    ; FO76 only counted once the waves had been told to spawn, and stopped counting at
    ; the stage that calls in the Motherlode.
    If !owningQuest.GetStageDone(iPreReqStage) || owningQuest.GetStageDone(iShutdownStage)
        Return
    EndIf

    Int living = CountLivingEnemies()
    If living <= 0 && !WavesStillSpawning(owningQuest)
        bRegistrationComplete = False
        owningQuest.SetStage(iShutdownStage)
        Return
    EndIf
    If living <= iEnemiesRemainingTarget && !owningQuest.GetStageDone(iStageToSet)
        ; Objective 25 swaps from the site marker to per-enemy markers at this stage.
        owningQuest.SetStage(iStageToSet)
    EndIf
EndFunction
