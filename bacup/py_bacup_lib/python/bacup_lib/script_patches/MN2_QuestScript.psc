; FO76's server ran Mischief Night. Lighting a pyre (200/300/400) sends JES-2R to the next one; its arrival
; (Phases[n].ReachedPyreStage) opens that area's Mischief Meter, mischief fills it, and the full meter sets
; MeterFilledStage then PhaseCompleteStage. The final pyre (500) brings the Assaultron bosses and the Vertibot.

Function StartEvent()
    Quest owner = Self as Quest
    ProgressBar = owner as Quests:_Default:ProgressBar:MasterScript
    EWS = owner as DefaultQuestEncounterWaveScript
    DEQ = owner as DefaultEventQuest
    bDoubleProgress = bLCPBonusOn && LCP_Bonus != None && LCP_Bonus.GetValue() > 0.0
    If ProgressBar != None
        RegisterForCustomEvent(ProgressBar, "ProgressBarFull")
        ProgressBar.HideProgressBar()
    EndIf
    If EWS != None
        RegisterForCustomEvent(EWS, "WaveComplete")
    EndIf
    If MN2_Weather != None
        MN2_Weather.SetActive(True, True)
    EndIf
    If EventActiveSnapshot != None
        EventActiveSnapshot.Push()
    EndIf
    SetDisplayVariable("AssaultronKills", 0)
EndFunction

Event OnQuestShutdown()
    CancelTimer(1)
    CancelTimer(2)
    If MN2_Weather != None
        Weather.ReleaseOverride()
    EndIf
    If EventActiveSnapshot != None
        EventActiveSnapshot.Remove()
    EndIf
    If ProgressBar != None
        UnregisterForCustomEvent(ProgressBar, "ProgressBarFull")
    EndIf
    If EWS != None
        UnregisterForCustomEvent(EWS, "WaveComplete")
        EWS.StopAllEncounterWaves()
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200 || auiStageID == 300 || auiStageID == 400
        ; The Eyebot's alias packages walk it to the next pyre; EyebotPathingFailsafeTimer covers a stuck path.
        StartTimer(EyebotPathingFailsafeTimer, 1)
    ElseIf auiStageID == 500
        If EWS != None
            EWS.StartEncounterWave(3)
        EndIf
        StartTimer(2.0, 2)
    EndIf
    If Phases == None
        Return
    EndIf
    Int index = 0
    While index < Phases.Length
        If auiStageID == Phases[index].ReachedPyreStage
            OpenMischiefMeter(index)
        ElseIf auiStageID == Phases[index].MeterFilledStage
            CloseMischiefMeter(index)
            SetStage(Phases[index].PhaseCompleteStage)
        EndIf
        index += 1
    EndWhile
EndEvent

Function OpenMischiefMeter(Int aiPhase)
    If ProgressBar != None
        ProgressBar.SetObjectiveIndex(30 + aiPhase * 2)
        ProgressBar.SetProgress(0.0)
        ProgressBar.DisplayProgressBar()
    EndIf
    If EWS != None
        EWS.StartEncounterWave(aiPhase)
    EndIf
EndFunction

Function CloseMischiefMeter(Int aiPhase)
    If ProgressBar != None
        ProgressBar.HideProgressBar()
    EndIf
    If EWS != None
        EWS.StopEncounterWave(aiPhase)
    EndIf
EndFunction

Int Function ActivePhase()
    If Phases == None
        Return -1
    EndIf
    Int index = 0
    While index < Phases.Length
        If IsStageDone(Phases[index].ReachedPyreStage) && !IsStageDone(Phases[index].MeterFilledStage)
            Return index
        EndIf
        index += 1
    EndWhile
    Return -1
EndFunction

Function AddMischief(Float afScore, Actor akPlayer)
    If ProgressBar == None || ActivePhase() < 0
        Return
    EndIf
    Float amount = afScore
    If bDoubleProgress
        amount *= 2.0
    EndIf
    If akPlayer != None && WearingKeyword != None && CostumeBonusMultiplier != None && akPlayer.WornHasKeyword(WearingKeyword)
        amount *= CostumeBonusMultiplier.GetValue()
    EndIf
    ProgressBar.ModProgress(amount)
EndFunction

Float Function ExplosionMultiplier()
    If bLCPExplosionsOn && LCP_Explosions != None && LCP_Explosions.GetValue() > 0.0 && ExplosionsCompensateMultiplier > 0.0
        Return ExplosionsCompensateMultiplier
    EndIf
    Return 1.0
EndFunction

Event Quests:_Default:ProgressBar:MasterScript.ProgressBarFull(Quests:_Default:ProgressBar:MasterScript akSender, Var[] akArgs)
    Int phase = ActivePhase()
    If phase >= 0
        SetStage(Phases[phase].MeterFilledStage)
    EndIf
EndEvent

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    Int phase = ActivePhase()
    If phase >= 0 && akArgs.Length > 0 && (akArgs[0] as Int) == phase
        EWS.StartEncounterWave(phase)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        ForceEyebotArrival()
    ElseIf aiTimerID == 2 && IsStageDone(500) && !IsStageDone(520)
        CountAssaultronKills()
        StartTimer(2.0, 2)
    EndIf
EndEvent

Function ForceEyebotArrival()
    If Phases == None
        Return
    EndIf
    Int index = 0
    While index < Phases.Length
        Int litStage = 200 + index * 100
        If IsStageDone(litStage) && !IsStageDone(Phases[index].ReachedPyreStage)
            ObjectReference eyebot = Alias_Actor_Eyebot.GetReference()
            If eyebot != None && Phases[index].PyreMarker != None && Phases[index].PyreMarker.GetReference() != None
                eyebot.MoveTo(Phases[index].PyreMarker.GetReference())
            EndIf
            SetStage(Phases[index].ReachedPyreStage)
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function CountAssaultronKills()
    RefCollectionAlias bosses = GetAlias(39) as RefCollectionAlias
    If bosses == None
        Return
    EndIf
    Int dead = 0
    Int index = 0
    While index < bosses.GetCount()
        Actor boss = bosses.GetAt(index) as Actor
        If boss != None && boss.IsDead()
            dead += 1
        EndIf
        index += 1
    EndWhile
    SetDisplayVariable("AssaultronKills", dead)
EndFunction

Function SetDisplayVariable(String asName, Float afValue)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(asName, afValue)
    EndIf
EndFunction
