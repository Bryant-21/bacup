Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    If !IsStageDone(2)
        SetStage(2)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 800
        StartArenaRoundClock(1)
    ElseIf auiStageID == 1200
        StartArenaRoundClock(2)
    ElseIf auiStageID == Round03GraftonAttackStage
        StartArenaRoundClock(3)
    ElseIf auiStageID == 810
        FinishArenaRoundClock(1)
    ElseIf auiStageID == 1210
        FinishArenaRoundClock(2)
    ElseIf auiStageID == 1610
        FinishArenaRoundClock(3)
    ElseIf auiStageID == 9001 || auiStageID == PlanBStage || auiStageID == PlanBFightStage
        StopArenaRoundClock()
    EndIf
    If auiStageID == PlanBFightStage || auiStageID == 1800
        StopCrowdSounds(False)
    EndIf
    If auiStageID == ChemCheatObjective
        StartChemClock()
    ElseIf auiStageID == ChemObjectiveSuccessStage || auiStageID == ChemObjectiveFailStage || auiStageID == 9001 || auiStageID == PlanBStage || auiStageID == PlanBFightStage
        StopChemClock()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender != Game.GetPlayer() || !IsRunning()
        Return
    EndIf
    If IsStageDone(ChemCheatObjective) && !chemClockActive
        StartChemClock()
    EndIf
    If activeRound != 0
        Return
    EndIf
    If IsStageDone(Round03GraftonAttackStage) && !IsStageDone(1610)
        StartArenaRoundClock(3)
    ElseIf IsStageDone(1200) && !IsStageDone(1210)
        StartArenaRoundClock(2)
    ElseIf IsStageDone(800) && !IsStageDone(810)
        StartArenaRoundClock(1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 80 && chemClockActive
        If IsRunning() && !IsStageDone(ChemObjectiveSuccessStage) && !IsStageDone(ChemObjectiveFailStage) && !IsStageDone(9001) && !IsStageDone(PlanBStage) && !IsStageDone(PlanBFightStage)
            SetChemTimeRemaining(0.0)
            StopChemClock()
            SetStage(ChemObjectiveFailStage)
        Else
            StopChemClock()
        EndIf
    ElseIf aiTimerID == 81 && chemClockActive
        If IsRunning() && !IsStageDone(ChemObjectiveSuccessStage) && !IsStageDone(ChemObjectiveFailStage)
            SetChemTimeRemaining(chemTimeRemaining - 1.0)
            StartTimer(1.0, 81)
        Else
            StopChemClock()
        EndIf
    ElseIf aiTimerID == 10000 + crowdTimerSerial
        StopCrowdSounds(True)
    ElseIf activeRound > 0 && aiTimerID == 60 + activeRound
        If IsRunning() && !IsStageDone(GetRoundFinishStage(activeRound)) && !IsStageDone(9001) && !IsStageDone(PlanBStage)
            SetRoundTimeRemaining(0.0)
            StopArenaRoundClock()
            SetStage(9001)
        EndIf
    ElseIf activeRound > 0 && aiTimerID == 70 + activeRound
        If IsRunning() && !IsStageDone(GetRoundFinishStage(activeRound)) && !IsStageDone(9001) && !IsStageDone(PlanBStage)
            SetRoundTimeRemaining(fRoundTime - 1.0)
            StartTimer(1.0, 70 + activeRound)
        Else
            StopArenaRoundClock()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    StopChemClock()
    StopArenaRoundClock()
    StopCrowdSounds(False)
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Function StartChemClock()
    If chemClockActive || !IsRunning() || !IsStageDone(ChemCheatObjective) || IsStageDone(ChemObjectiveSuccessStage) || IsStageDone(ChemObjectiveFailStage)
        Return
    EndIf
    If IsStageDone(9001) || IsStageDone(PlanBStage) || IsStageDone(PlanBFightStage)
        Return
    EndIf
    chemClockActive = True
    SetChemTimeRemaining(180.0)
    StartTimer(180.0, 80)
    StartTimer(1.0, 81)
EndFunction

Function SetChemTimeRemaining(Float afRemaining)
    If afRemaining < 0.0
        afRemaining = 0.0
    EndIf
    chemTimeRemaining = afRemaining
    If W05_MQR_203P_Round03CheatObjectiveTimer != None
        W05_MQR_203P_Round03CheatObjectiveTimer.SetValue(afRemaining)
        UpdateCurrentInstanceGlobal(W05_MQR_203P_Round03CheatObjectiveTimer)
    EndIf
EndFunction

Function StopChemClock()
    chemClockActive = False
    CancelTimer(80)
    CancelTimer(81)
EndFunction

Int Function GetRoundFinishStage(Int aiRound)
    If aiRound == 1
        Return 810
    ElseIf aiRound == 2
        Return 1210
    EndIf
    Return 1610
EndFunction

GlobalVariable Function GetRoundTimeGlobal(Int aiRound)
    If aiRound == 1
        Return W05_MQR_203P_Round01ObjectiveTimer
    ElseIf aiRound == 2
        Return W05_MQR_203P_Round02ObjectiveTimer
    EndIf
    Return W05_MQR_203P_Round03ObjectiveTimer
EndFunction

Function StartArenaRoundClock(Int aiRound)
    If !IsRunning() || activeRound == aiRound || IsStageDone(9001) || IsStageDone(PlanBStage) || IsStageDone(PlanBFightStage)
        Return
    EndIf
    If aiRound < 1 || aiRound > 3 || IsStageDone(GetRoundFinishStage(aiRound))
        Return
    EndIf
    StopArenaRoundClock()
    activeRound = aiRound
    roundClockUnlimited = False
    fRoundTotalTime = 360.0 - aiRound * 60.0
    If aiRound == 3 && currentPlayer != None && W05_MQR_203P_Round3CheatValue != None
        Actor playerRef = currentPlayer.GetActorReference()
        If playerRef != None && IsStageDone(MaddieObjectiveSuccessStage) && playerRef.GetValue(W05_MQR_203P_Round3CheatValue) == iCheatValueMaddie
            roundClockUnlimited = True
            SetObjectiveDisplayed(Round03TimeObjective, False)
            SetObjectiveDisplayed(Round03FightObjective)
        EndIf
    EndIf
    If !roundClockUnlimited
        SetRoundTimeRemaining(fRoundTotalTime)
        StartTimer(fRoundTotalTime, 60 + aiRound)
        StartTimer(1.0, 70 + aiRound)
    EndIf
EndFunction

Function SetRoundTimeRemaining(Float afRemaining)
    If afRemaining < 0.0
        afRemaining = 0.0
    EndIf
    fRoundTime = afRemaining
    GlobalVariable displayGlobal = GetRoundTimeGlobal(activeRound)
    If displayGlobal != None
        displayGlobal.SetValue(afRemaining)
        UpdateCurrentInstanceGlobal(displayGlobal)
    EndIf
EndFunction

Function FinishArenaRoundClock(Int aiRound)
    If activeRound == aiRound
        StopArenaRoundClock()
    EndIf
    If IsStageDone(9001) || IsStageDone(PlanBStage) || IsStageDone(PlanBFightStage)
        Return
    EndIf
    If aiRound == 1
        SetObjectiveCompleted(Round01FightObjective)
        SetObjectiveCompleted(Round01TimeObjective)
    ElseIf aiRound == 2
        SetObjectiveCompleted(Round02FightObjective)
        SetObjectiveCompleted(Round02TimeObjective)
    ElseIf aiRound == 3
        SetObjectiveCompleted(Round03FightObjective)
        SetObjectiveCompleted(Round03TimeObjective)
    EndIf
EndFunction

Function StopArenaRoundClock()
    If activeRound > 0
        CancelTimer(60 + activeRound)
        CancelTimer(70 + activeRound)
    EndIf
    activeRound = 0
    roundClockUnlimited = False
EndFunction

Function PlayCrowdReaction(Int aiMarkerAlias)
    If !IsRunning() || IsStageDone(PlanBFightStage) || IsStageDone(1800) || SoundMarkerData == None
        Return
    EndIf
    ReferenceAlias reactionAlias = GetAlias(aiMarkerAlias) as ReferenceAlias
    If reactionAlias == None
        Return
    EndIf
    Int soundIndex = SoundMarkerData.FindStruct("SoundMarker", reactionAlias)
    ObjectReference markerRef = reactionAlias.GetReference()
    If soundIndex < 0 || markerRef == None
        Return
    EndIf
    Float duration = SoundMarkerData[soundIndex].TimerLength
    If duration <= 0.0
        Return
    EndIf
    StopCrowdSounds(False)
    crowdTimerSerial += 1
    CurrentSoundMarker = markerRef
    CurrentSoundMarkerTimerLength = duration
    markerRef.EnableNoWait()
    StartTimer(duration, 10000 + crowdTimerSerial)
EndFunction

Function StopCrowdSounds(Bool abRestoreIdle = False)
    CancelTimer(10000 + crowdTimerSerial)
    If SoundMarkerData != None
        Int i = 0
        While i < SoundMarkerData.Length
            If SoundMarkerData[i].SoundMarker != None
                ObjectReference markerRef = SoundMarkerData[i].SoundMarker.GetReference()
                If markerRef != None
                    markerRef.DisableNoWait()
                EndIf
            EndIf
            i += 1
        EndWhile
    EndIf
    CurrentSoundMarker = None
    If CrowdIdleLowMarker != None
        ObjectReference ambientRef = CrowdIdleLowMarker.GetReference()
        If ambientRef != None
            If abRestoreIdle && IsRunning() && !IsStageDone(PlanBFightStage) && !IsStageDone(1800)
                ambientRef.EnableNoWait()
            Else
                ambientRef.DisableNoWait()
            EndIf
        EndIf
    EndIf
EndFunction
