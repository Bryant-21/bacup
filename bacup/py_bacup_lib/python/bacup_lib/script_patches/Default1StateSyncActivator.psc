State Waiting
    Event OnActivate(ObjectReference akActionRef)
        GoToState("animating")
    EndEvent
EndState

State animating
    Event OnBeginState(String asOldState)
        IsPlayingSyncAnimation = True
        SetAnimationVariableFloat(SyncAnimProgressVariable, 0.0)
        PlayAnimation(SyncAnimName)
        If SyncAnimDuration > 0.0
            StartTimer(SyncAnimDuration, CONST_AnimationEndEventID)
        EndIf
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != CONST_AnimationEndEventID
            Return
        EndIf

        IsPlayingSyncAnimation = False
        If ShouldAutoReset
            SetAnimationVariableFloat(SyncAnimProgressVariable, 0.0)
            GoToState("Waiting")
        EndIf
    EndEvent
EndState
