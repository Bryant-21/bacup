Function InitializeLocalState()
    NetState_Open = 1
    NetState_Opening = 2
    NetState_Closed = 3
    NetState_Closing = 4
    NetState_BlockedClosed = 5
    NetState_BlockedOpen = 6

    If OpenAnim == ""
        OpenAnim = "Play01"
    EndIf
    If CloseAnim == ""
        CloseAnim = "Play02"
    EndIf
    If SetClosedAnim == ""
        SetClosedAnim = "JumpState01"
    EndIf
    If SetOpenAnim == ""
        SetOpenAnim = "JumpState02"
    EndIf
    If OpenAnimEventName == ""
        OpenAnimEventName = "Done"
    EndIf
    If CloseAnimEventName == ""
        CloseAnimEventName = "Done"
    EndIf
    If OpenState <= 0
        OpenState = NetState_Closed
    EndIf
EndFunction

Function ApplyLocalPresentation(Bool openState)
    Bool forceBlocked = OpenState == NetState_BlockedClosed || OpenState == NetState_BlockedOpen
    If forceBlocked
        BlockActivation(True, True)
        IsBlockingActivations = True
    ElseIf EnableBlockActivation
        If openState
            BlockActivation(BlockActivationOpen, BlockActivationHideTextOpen)
            IsBlockingActivations = BlockActivationOpen
        Else
            BlockActivation(BlockActivationClosed, BlockActivationHideTextClosed)
            IsBlockingActivations = BlockActivationClosed
        EndIf
    ElseIf IsBlockingActivations
        BlockActivation(False, False)
        IsBlockingActivations = False
    EndIf

    If EnableOverrideDisplayName
        If openState
            SetOverrideName(OverrideDisplayNameOpen)
        Else
            SetOverrideName(OverrideDisplayNameClosed)
        EndIf
    EndIf
    If EnableOverrideActivateText
        If openState
            SetActivateTextOverride(OverrideActivateTextOpen)
        Else
            SetActivateTextOverride(OverrideActivateTextClosed)
        EndIf
    EndIf

    If EnableLinkedRefChainToggling && TwoStateCollisionKeyword != None
        Bool disableChain = openState != InvertCollision
        If disableChain
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
    EndIf
EndFunction

Function SetLocalOpen(Bool openState = True, Bool playTransition = True)
    If lock_UpdateNetworkState
        Return
    EndIf
    lock_UpdateNetworkState = True
    CancelTimer(CONST_AutoCloseTimerID)
    InitializeLocalState()

    Bool changed = IsOpen != openState
    If changed && playTransition && Is3DLoaded()
        If openState
            OpenState = NetState_Opening
            If OpenAnimEventName != ""
                PlayAnimationAndWait(OpenAnim, OpenAnimEventName)
            Else
                PlayAnimation(OpenAnim)
            EndIf
        Else
            OpenState = NetState_Closing
            If CloseAnimEventName != ""
                PlayAnimationAndWait(CloseAnim, CloseAnimEventName)
            Else
                PlayAnimation(CloseAnim)
            EndIf
        EndIf
    EndIf

    IsOpen = openState
    If openState
        OpenState = NetState_Open
        GoToState("Open")
    Else
        OpenState = NetState_Closed
        GoToState("Closed")
    EndIf
    ApplyLocalPresentation(openState)

    If changed
        hasDoneOnce = True
    EndIf
    If openState && changed && ShouldAutoClose
        Float closeDelay = AutoCloseDelay
        If closeDelay <= 0.0 && DefaultAutoCloseDelay != None
            closeDelay = DefaultAutoCloseDelay.GetValue()
        EndIf
        If closeDelay <= 0.0
            closeDelay = 5.0
        EndIf
        StartTimer(closeDelay, CONST_AutoCloseTimerID)
    EndIf
    lock_UpdateNetworkState = False
EndFunction

Function SetLocalBlocked(Bool blocked, Bool openState)
    InitializeLocalState()
    If blocked
        IsOpen = openState
        If openState
            OpenState = NetState_BlockedOpen
            GoToState("BlockedOpen")
        Else
            OpenState = NetState_BlockedClosed
            GoToState("BlockedClosed")
        EndIf
        ApplyLocalPresentation(openState)
    Else
        SetLocalOpen(openState, False)
    EndIf
EndFunction

Function UpdateNetworkState(Bool isClientUpdateOnLoad)
    InitializeLocalState()
    If OpenState == NetState_BlockedClosed
        SetLocalBlocked(True, False)
    ElseIf OpenState == NetState_BlockedOpen
        SetLocalBlocked(True, True)
    Else
        SetLocalOpen(IsOpen, False)
    EndIf
    If isClientUpdateOnLoad
        Private_UpdateOnClientLoad()
    EndIf
EndFunction

Event OnInit()
    InitializeLocalState()
    SetLocalOpen(IsOpen, False)
EndEvent

Event OnSimpleNetworkStateSet()
    UpdateNetworkState(False)
EndEvent

Event OnLoad()
    UpdateNetworkState(True)
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If lock_UpdateNetworkState || IsBlockingActivations
        Return
    EndIf
    If ShouldDoOnce && hasDoneOnce
        Return
    EndIf
    SetLocalOpen(!IsOpen, True)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_AutoCloseTimerID && IsOpen
        SetLocalOpen(False, True)
    EndIf
EndEvent

Event OnReset()
    hasDoneOnce = False
    SetLocalBlocked(False, False)
EndEvent
