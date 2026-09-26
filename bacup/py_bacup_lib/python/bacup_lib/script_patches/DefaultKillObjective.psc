Event OnQuestInit()
    currentStageIndexes = new Int[0]
    CurrentActorsToKillArray = new ActorandAmount[0]
    B21CountedVictims = new Actor[0]
    Int index = 0
    While ActorsToKillArray != None && index < ActorsToKillArray.Length
        If ActorsToKillArray[index] != None
            ActorsToKillArray[index].KillCount = 0
        EndIf
        index += 1
    EndWhile
    index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        If SceneTriggersArray[index] != None
            SceneTriggersArray[index].bDoneOnce = False
        EndIf
        index += 1
    EndWhile
    RegisterDeathSources()
    RefreshStageWindows()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshStageWindows()
EndEvent

Event RefCollectionAlias.OnDeath(RefCollectionAlias akSender, ObjectReference akSenderRef, Actor akKiller)
    HandleActorDeath(akSenderRef as Actor, akKiller)
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    HandleActorDeath(akVictim, akSender)
EndEvent

; Wave actors can die to anything, so every wave collection is watched; the player covers non-wave kills.
Function RegisterDeathSources()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnKill")
    EndIf
    Int index = 0
    While StagesProperties != None && index < StagesProperties.Length
        If StagesProperties[index] != None && StagesProperties[index].EWSRefCollection != None
            RegisterForRemoteEvent(StagesProperties[index].EWSRefCollection, "OnDeath")
        EndIf
        index += 1
    EndWhile
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves == None || waves.EncounterWaves == None
        Return
    EndIf
    index = 0
    While index < waves.EncounterWaves.Length
        DefaultQuestEncounterWaveScript:EncounterWaveData wave = waves.EncounterWaves[index]
        If wave != None
            If wave.WaveRefCollection != None
                RegisterForRemoteEvent(wave.WaveRefCollection, "OnDeath")
            EndIf
            If wave.WaveRefCollectionSecondary != None
                RegisterForRemoteEvent(wave.WaveRefCollectionSecondary, "OnDeath")
            EndIf
            If wave.BossRefCollection != None
                RegisterForRemoteEvent(wave.BossRefCollection, "OnDeath")
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function StageWindowOpen(Int aiStageIndex)
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage == None || !IsStageDone(stage.preReqStage)
        Return False
    EndIf
    If stage.EndStage > 0 && IsStageDone(stage.EndStage)
        Return False
    EndIf
    Return stage.StageToSet < 0 || !IsStageDone(stage.StageToSet)
EndFunction

Function RefreshStageWindows()
    If StagesProperties == None
        Return
    EndIf
    If currentStageIndexes == None
        currentStageIndexes = new Int[0]
    EndIf
    If CurrentActorsToKillArray == None
        CurrentActorsToKillArray = new ActorandAmount[0]
    EndIf
    Int index = 0
    While index < StagesProperties.Length
        Bool open = StageWindowOpen(index)
        Int slot = currentStageIndexes.Find(index)
        If open && slot < 0
            currentStageIndexes.Add(index)
            StartTrackingStage(index)
        ElseIf !open && slot >= 0
            currentStageIndexes.Remove(slot)
            StopTrackingStage(index)
        EndIf
        index += 1
    EndWhile
EndFunction

Function StartTrackingStage(Int aiStageIndex)
    Int index = 0
    While ActorsToKillArray != None && index < ActorsToKillArray.Length
        ActorandAmount target = ActorsToKillArray[index]
        If target != None && target.StagesIndex == aiStageIndex
            If CurrentActorsToKillArray.Find(target) < 0
                CurrentActorsToKillArray.Add(target)
            EndIf
            SetQuestVariable(target.KillGoalTextReplacementString, target.AmountToKill)
            UpdateKillText(target, StagesProperties[aiStageIndex])
        EndIf
        index += 1
    EndWhile
    CheckStageComplete(aiStageIndex)
EndFunction

Function StopTrackingStage(Int aiStageIndex)
    Int index = CurrentActorsToKillArray.Length - 1
    While index >= 0
        If CurrentActorsToKillArray[index] == None || CurrentActorsToKillArray[index].StagesIndex == aiStageIndex
            CurrentActorsToKillArray.Remove(index)
        EndIf
        index -= 1
    EndWhile
    StagesStruct stage = StagesProperties[aiStageIndex]
    ; Only the dead are dropped so living wave actors keep their alias data.
    If stage != None && stage.bCleanUpEWSRefCollOnEnd && stage.EWSRefCollection != None
        index = stage.EWSRefCollection.GetCount() - 1
        While index >= 0
            Actor member = stage.EWSRefCollection.GetAt(index) as Actor
            If member != None && member.IsDead()
                stage.EWSRefCollection.RemoveRef(member)
            EndIf
            index -= 1
        EndWhile
    EndIf
EndFunction

Bool Function KillCredited(StagesStruct akStage, Actor akKiller)
    Actor stageActor = None
    If akStage.ActorAlias != None
        stageActor = akStage.ActorAlias.GetActorReference()
    EndIf
    If akStage.bIgnoreEventPlayerKills
        Return stageActor != None && akKiller == stageActor
    EndIf
    If akStage.bOnlyCountPlayerOrActorKills
        Return akKiller != None && (akKiller == Game.GetPlayer() || akKiller == stageActor)
    EndIf
    Return True
EndFunction

Function HandleActorDeath(Actor akVictim, Actor akKiller)
    If akVictim == None || currentStageIndexes == None || currentStageIndexes.Length == 0
        Return
    EndIf
    If B21CountedVictims == None
        B21CountedVictims = new Actor[0]
    EndIf
    If B21CountedVictims.Find(akVictim) >= 0
        Return
    EndIf
    If B21CountedVictims.Length >= 120
        B21CountedVictims.Remove(0)
    EndIf
    B21CountedVictims.Add(akVictim)
    ActorBase victimBase = akVictim.GetActorBase()
    ActorBase victimLeveledBase = akVictim.GetLeveledActorBase()
    ; Completing a stage re-enters RefreshStageWindows, so iterate over a copy of the open windows.
    Int[] openStages = new Int[0]
    Int stageSlot = 0
    While stageSlot < currentStageIndexes.Length
        openStages.Add(currentStageIndexes[stageSlot])
        stageSlot += 1
    EndWhile
    stageSlot = 0
    While stageSlot < openStages.Length
        Int stageIndex = openStages[stageSlot]
        StagesStruct stage = StagesProperties[stageIndex]
        If stage != None && KillCredited(stage, akKiller)
            Int index = 0
            While index < ActorsToKillArray.Length
                ActorandAmount target = ActorsToKillArray[index]
                If target != None && target.StagesIndex == stageIndex && target.ActorToKill != None && (target.ActorToKill == victimBase || target.ActorToKill == victimLeveledBase)
                    target.KillCount += 1
                    UpdateKillText(target, stage)
                    CheckSceneTriggers(target)
                EndIf
                index += 1
            EndWhile
            CheckStageComplete(stageIndex)
        EndIf
        stageSlot += 1
    EndWhile
EndFunction

Function UpdateKillText(ActorandAmount akTarget, StagesStruct akStage)
    Int shown = akTarget.KillCount
    If shown > akTarget.AmountToKill && (akStage == None || !akStage.bAllowUIOverflow)
        shown = akTarget.AmountToKill
    EndIf
    SetQuestVariable(akTarget.KillCountTextReplacementString, shown)
EndFunction

Function CheckStageComplete(Int aiStageIndex)
    StagesStruct stage = StagesProperties[aiStageIndex]
    If stage == None || stage.StageToSet < 0 || IsStageDone(stage.StageToSet)
        Return
    EndIf
    Bool anyTarget = False
    Int index = 0
    While ActorsToKillArray != None && index < ActorsToKillArray.Length
        ActorandAmount target = ActorsToKillArray[index]
        If target != None && target.StagesIndex == aiStageIndex
            anyTarget = True
            If target.KillCount < target.AmountToKill
                Return
            EndIf
        EndIf
        index += 1
    EndWhile
    If anyTarget
        SetStage(stage.StageToSet)
    EndIf
EndFunction

Function CheckSceneTriggers(ActorandAmount akChanged)
    Int index = 0
    While SceneTriggersArray != None && index < SceneTriggersArray.Length
        SceneTriggerStruct sceneTrigger = SceneTriggersArray[index]
        If sceneTrigger != None && sceneTrigger.SceneToTrigger != None && !(sceneTrigger.bDoOnce && sceneTrigger.bDoneOnce)
            Int count = -1
            If sceneTrigger.TextReplacementStringRelatedTo != "" && sceneTrigger.TextReplacementStringRelatedTo == akChanged.KillCountTextReplacementString
                count = akChanged.KillCount
            ElseIf sceneTrigger.TextReplacementStringRelatedTo == "" && sceneTrigger.ActorRelatedTo != None && sceneTrigger.ActorRelatedTo == akChanged.ActorToKill
                count = akChanged.KillCount
            EndIf
            If count >= 0 && CompareCount(count, sceneTrigger.comparingOperator, sceneTrigger.CountToTriggerAt)
                sceneTrigger.bDoneOnce = True
                If sceneTrigger.bForceScene
                    sceneTrigger.SceneToTrigger.ForceStart()
                Else
                    sceneTrigger.SceneToTrigger.Start()
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function CompareCount(Int aiValue, String asOperator, Int aiTarget)
    If asOperator == ">"
        Return aiValue > aiTarget
    ElseIf asOperator == "<"
        Return aiValue < aiTarget
    ElseIf asOperator == "<=" || asOperator == "=<"
        Return aiValue <= aiTarget
    ElseIf asOperator == "==" || asOperator == "="
        Return aiValue == aiTarget
    ElseIf asOperator == "!="
        Return aiValue != aiTarget
    EndIf
    Return aiValue >= aiTarget
EndFunction

Function SetQuestVariable(String asName, Int aiValue)
    If asName == ""
        Return
    EndIf
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asName, aiValue as Float)
    EndIf
EndFunction
