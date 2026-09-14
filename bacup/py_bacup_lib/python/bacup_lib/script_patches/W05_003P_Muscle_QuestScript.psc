Event OnQuestInit()
    If !IsStageDone(10)
        SetStage(10)
    EndIf
    If !IsStageDone(iFirstTimeSceneStage)
        SetStage(iFirstTimeSceneStage)
    EndIf
    StartTimer(fRadioCheckTimerLength, iRadioCheckTimerID)
EndEvent

Event OnQuestShutdown()
    CancelTimer(iRadioCheckTimerID)
    If MUS76CombatW05ScorchedAttack
        MUS76CombatW05ScorchedAttack.Remove()
    EndIf
EndEvent

; FO76 polled here to see whether the player had tuned to the tracking station,
; then swapped the "tune your radio" objective for "find the crew". The polling
; body was stripped server-side, so the signal never came on air and the quest
; stalled at stage 150. Rebuilt against FO4's own radio API — Game.IsPlayerListening
; is exactly what BoSM01DistressPulserAliasScript uses for the same job — so this
; is behavioural parity, not a substitute.
Event OnTimer(Int aiTimerID)
    If aiTimerID != iRadioCheckTimerID
        Return
    EndIf

    ; Terminal: the quest has moved past the radio leg. Do not re-arm.
    If IsStageDone(iRadioShutdownStage) || IsObjectiveCompleted(iTuneToRadioObj)
        Return
    EndIf

    ; Watchdog. The radio quest is event-scoped and is started by this quest's
    ; stage-10 fragment; re-send if it never came up ("keyword used to restart the
    ; radio quest if the player didn't get one for some reason").
    If W05_MQ_003P_Radio && W05_MQ_003P_Muscle_Radio_QuestStartKeyword
        If !W05_MQ_003P_Radio.IsRunning() && !W05_MQ_003P_Radio.IsCompleted()
            Actor playerRef = owningPlayer.GetActorReference()
            W05_MQ_003P_Muscle_Radio_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef, 0, 0)
        EndIf
    EndIf

    If Game.IsPlayerListening(fTrackingRadio)
        SetObjectiveCompleted(iTuneToRadioObj)
        If !IsObjectiveDisplayed(iFindtheCrewObj)
            SetObjectiveDisplayed(iFindtheCrewObj)
        EndIf
        Return
    EndIf

    StartTimer(fRadioCheckTimerLength, iRadioCheckTimerID)
EndEvent

Function StartLocalCombatMusic()
    If MUS76CombatW05ScorchedAttack
        MUS76CombatW05ScorchedAttack.Add()
    EndIf
EndFunction

Function StopLocalCombatMusic()
    If MUS76CombatW05ScorchedAttack
        MUS76CombatW05ScorchedAttack.Remove()
    EndIf
EndFunction
