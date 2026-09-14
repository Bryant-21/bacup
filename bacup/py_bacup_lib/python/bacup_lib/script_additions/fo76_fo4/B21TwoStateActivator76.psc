Scriptname B21TwoStateActivator76 extends Default2StateActivator

String Property SetClosedAnim = "JumpState01" Auto
String Property SetOpenAnim = "JumpState02" Auto
String Property OpenAnimEventName = "Done" Auto
String Property CloseAnimEventName = "Done" Auto
Bool Property ShouldDoOnce = False Auto
Bool Property hasDoneOnce = False Auto Hidden
Float Property DelayBeforeActivationsAllowed = 0.0 Auto
Bool Property DoNotAnimateIfActivationBlocked = False Auto
Float Property AutoCloseDelay = -1.0 Auto
Bool Property ShouldAutoClose = False Auto
Bool Property IsSynced = False Auto Hidden Conditional
Int Property OpenState = 3 Auto Hidden Conditional
GlobalVariable Property DefaultAutoCloseDelay Auto

Int Property CONST_OpenTimerID = 1 AutoReadOnly
Int Property CONST_CloseTimerID = 2 AutoReadOnly
Int Property CONST_AutoCloseTimerID = 3 AutoReadOnly

Function PrepareFO76Defaults()
    If openAnim == "" || openAnim == "open"
        openAnim = "Play01"
    EndIf
    If closeAnim == "" || closeAnim == "close"
        closeAnim = "Play02"
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

    openEvent = OpenAnimEventName
    closeEvent = CloseAnimEventName
    startOpenAnim = SetOpenAnim
    doOnce = ShouldDoOnce
EndFunction

Function SetDefaultState()
    PrepareFO76Defaults()
    isAnimating = False
    If isOpen
        PlayAnimation(SetOpenAnim)
        If InvertCollision
            EnableLinkChain(TwoStateCollisionKeyword)
        Else
            DisableLinkChain(TwoStateCollisionKeyword)
        EndIf
        myState = 0
        OpenState = 1
    Else
        PlayAnimation(SetClosedAnim)
        If InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
        myState = 1
        OpenState = 3
    EndIf
EndFunction

Function SetOpen(Bool abOpen = True)
    PrepareFO76Defaults()
    Bool changed = isOpen != abOpen
    Parent.SetOpen(abOpen)

    If isOpen
        OpenState = 1
    Else
        OpenState = 3
    EndIf
    If changed
        hasDoneOnce = True
    EndIf

    CancelTimer(CONST_AutoCloseTimerID)
    If isOpen && ShouldAutoClose
        StartAutoCloseTimer()
    EndIf
EndFunction

Function SetActivatorOpenAndWait(Bool abOpen)
    SetOpen(abOpen)
EndFunction

Function SetActivatorOpen(Bool abOpen)
    PrepareFO76Defaults()
    SetOpenNoWait(abOpen)
EndFunction

Function StartAutoCloseTimer()
    If !ShouldAutoClose
        Return
    EndIf

    Float closeDelay = AutoCloseDelay
    If closeDelay <= 0.0 && DefaultAutoCloseDelay != None
        closeDelay = DefaultAutoCloseDelay.GetValue()
    EndIf
    If closeDelay > 0.0
        StartTimer(closeDelay, CONST_AutoCloseTimerID)
    EndIf
EndFunction

Function SetAutoClose(Bool abAutoClose)
    ShouldAutoClose = abAutoClose
    CancelTimer(CONST_AutoCloseTimerID)
    If isOpen && ShouldAutoClose
        StartAutoCloseTimer()
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_AutoCloseTimerID
        If isOpen
            SetOpen(False)
        EndIf
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent

Event OnReset()
    hasDoneOnce = False
    Parent.OnReset()
EndEvent
