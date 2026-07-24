Function SetActivatorOpen(Bool abOpen)
    If ShouldDoOnce && hasDoneOnce
        Return
    EndIf

    String currentState = GetState()
    If abOpen && !IsOpen && currentState != "opening"
        GoToState("opening")
    ElseIf !abOpen && IsOpen && currentState != "closing"
        GoToState("closing")
    EndIf
EndFunction

Function SetActivatorOpenAndWait(Bool abOpen)
    SetActivatorOpen(abOpen)
    While GetState() == "opening" || GetState() == "closing"
        Utility.Wait(0.1)
    EndWhile
EndFunction

Function StartAutoCloseTimer()
    If !ShouldAutoClose
        Return
    EndIf

    Float delay = AutoCloseDelay
    If delay <= 0.0
        delay = DefaultAutoCloseDelay.GetValue()
    EndIf
    StartTimer(delay, CONST_AutoCloseTimerID)
EndFunction

Function ReconcileSyncState()
    CancelTimer(CONST_OpenTimerID)
    CancelTimer(CONST_CloseTimerID)
    CancelTimer(CONST_AutoCloseTimerID)

    If IsOpen
        GoToState("startsopen")
    Else
        GoToState("startsclosed")
    EndIf
EndFunction

Event OnInit()
    ReconcileSyncState()
EndEvent

Event OnLoad()
    ReconcileSyncState()
EndEvent

Event OnReset()
    ReconcileSyncState()
EndEvent

Event OnTimer(Int aiTimerID)
    ; Defensive fallback only: opening/closing/open each override OnTimer for
    ; their own pending IDs. This only catches a timer that outlives a
    ; ReconcileSyncState() cancellation race.
EndEvent

State opening
    Event OnBeginState(String asOldState)
        PlaySyncSFXOnClients()
        If SyncOpenProgressVariable != ""
            SetAnimationVariableFloat(SyncOpenProgressVariable, 0.0)
        EndIf
        If SyncOpenAnim != ""
            PlayAnimation(SyncOpenAnim)
        EndIf
        StartTimer(SyncOpenDuration, CONST_OpenTimerID)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != CONST_OpenTimerID
            Return
        EndIf

        If InvertCollision
            EnableLinkChain(TwoStateCollisionKeyword)
        Else
            DisableLinkChain(TwoStateCollisionKeyword)
        EndIf
        If SyncOpenProgressVariable != ""
            SetAnimationVariableFloat(SyncOpenProgressVariable, 1.0)
        EndIf

        IsOpen = True
        OpenState = 0
        If ShouldDoOnce
            hasDoneOnce = True
        EndIf
        GoToState("open")
    EndEvent
EndState

State closing
    Event OnBeginState(String asOldState)
        PlaySyncSFXOnClients()
        If SyncCloseProgressVariable != ""
            SetAnimationVariableFloat(SyncCloseProgressVariable, 0.0)
        EndIf
        If SyncCloseAnim != ""
            PlayAnimation(SyncCloseAnim)
        EndIf
        StartTimer(SyncCloseDuration, CONST_CloseTimerID)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != CONST_CloseTimerID
            Return
        EndIf

        If InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
        If SyncCloseProgressVariable != ""
            SetAnimationVariableFloat(SyncCloseProgressVariable, 1.0)
        EndIf

        IsOpen = False
        OpenState = 1
        If ShouldDoOnce
            hasDoneOnce = True
        EndIf
        GoToState("closed")
    EndEvent
EndState

State open
    Event OnBeginState(String asOldState)
        StartAutoCloseTimer()
    EndEvent

    Event OnEndState(String asNewState)
        CancelTimer(CONST_AutoCloseTimerID)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == CONST_AutoCloseTimerID
            SetActivatorOpen(False)
        EndIf
    EndEvent
EndState

State startsopen
    Event OnBeginState(String asOldState)
        If InvertCollision
            EnableLinkChain(TwoStateCollisionKeyword)
        Else
            DisableLinkChain(TwoStateCollisionKeyword)
        EndIf
        If SyncOpenAnim != ""
            PlayAnimation(SyncOpenAnim)
        EndIf
        OpenState = 0
        GoToState("open")
    EndEvent
EndState

State startsclosed
    Event OnBeginState(String asOldState)
        If InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
        If SyncCloseAnim != ""
            PlayAnimation(SyncCloseAnim)
        EndIf
        OpenState = 1
        GoToState("closed")
    EndEvent
EndState
