Function BeginSyncAnimation()
    If GetState() == "Waiting"
        GoToState("animating")
    EndIf
EndFunction

Float Function GetSyncAnimationProgress()
    Return B21SyncProgress
EndFunction

Function ResetSyncAnimation()
    CancelTimer(CONST_AnimationEndEventID)
    IsPlayingSyncAnimation = False
    B21SyncProgress = 0.0
    If Is3DLoaded()
        SetAnimationVariableFloat(SyncAnimProgressVariable, B21SyncProgress)
    EndIf
    GoToState("Waiting")
EndFunction

Event OnReset()
    ResetSyncAnimation()
EndEvent

Event OnLoad()
    If GetState() == "animating"
        If IsPlayingSyncAnimation
            StartTimer(0.05, CONST_AnimationEndEventID)
        Else
            B21SyncProgress = 1.0
        EndIf
        PlayAnimation(SyncAnimName)
    EndIf
    SetAnimationVariableFloat(SyncAnimProgressVariable, B21SyncProgress)
EndEvent

State Waiting
    Event OnActivate(ObjectReference akActionRef)
        BeginSyncAnimation()
    EndEvent
EndState

State animating
    Event OnBeginState(String asOldState)
        IsPlayingSyncAnimation = True
        B21SyncProgress = 0.0
        If Is3DLoaded()
            SetAnimationVariableFloat(SyncAnimProgressVariable, B21SyncProgress)
            PlayAnimation(SyncAnimName)
        EndIf
        StartTimer(0.05, CONST_AnimationEndEventID)
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != CONST_AnimationEndEventID || !IsPlayingSyncAnimation
            Return
        EndIf
        ; Timer time excludes paused menus and survives saves; wall time does neither.
        If SyncAnimDuration > 0.0
            B21SyncProgress += 0.05 / SyncAnimDuration
        Else
            B21SyncProgress = 1.0
        EndIf
        If B21SyncProgress >= 1.0
            B21SyncProgress = 1.0
            IsPlayingSyncAnimation = False
        EndIf
        If Is3DLoaded()
            SetAnimationVariableFloat(SyncAnimProgressVariable, B21SyncProgress)
        EndIf
        If IsPlayingSyncAnimation
            StartTimer(0.05, CONST_AnimationEndEventID)
        ElseIf ShouldAutoReset
            ResetSyncAnimation()
        EndIf
    EndEvent
EndState
