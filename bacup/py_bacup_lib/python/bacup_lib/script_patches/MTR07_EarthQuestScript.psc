Event OnStageSet(Int auiStageID, Int auiItemID)
    CheckCoreInventory()
    If auiStageID == 200
        RegisterForPlayerLoadReconciliation()
        EnsureAttackTimer()
        ; ToggleButtonScript re-activates itself after its own press animation.
        StartTimer(3.0, 901)
    ElseIf auiStageID == 255
        CancelTimer(AttackTimerID)
        B21WheelTimerStarted = False
    EndIf
    ApplyMachineState()
    If auiStageID == 255
        ; Stopping after rewards lets the console terminal start the next run.
        StartTimer(5.0, 902)
    EndIf
EndEvent

Event OnQuestInit()
    B21WheelTimerStarted = False
    RegisterForPlayerLoadReconciliation()
    If GetStage() < 10 && !IsStageDone(10)
        SetStage(10)
    EndIf
    EnsureAttackTimer()
    CheckCoreInventory()
    ApplyMachineState()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
    CancelTimer(AttackTimerID)
    CancelTimer(901)
    CancelTimer(902)
    B21WheelTimerStarted = False
    SetMachineIdle()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EnsureAttackTimer()
        RegisterForPlayerLoadReconciliation()
        CheckCoreInventory()
        ApplyMachineState()
        If IsRunning() && IsStageDone(255)
            StartTimer(5.0, 902)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Quest earthQuest = Self as Quest
    If aiTimerID == AttackTimerID && earthQuest.IsRunning() && earthQuest.IsStageDone(200) && !earthQuest.IsStageDone(255)
        earthQuest.SetStage(255)
    ElseIf aiTimerID == 901
        UpdatePowerButton()
    ElseIf aiTimerID == 902 && earthQuest.IsRunning() && earthQuest.IsStageDone(255)
        earthQuest.Stop()
    EndIf
EndEvent

Function RegisterForPlayerLoadReconciliation()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        RegisterForRemoteEvent(playerRef, "OnItemAdded")
        If IgnitionReactorCore01 != None
            AddInventoryEventFilter(IgnitionReactorCore01)
        EndIf
    EndIf
    ObjectReference powerButton = Alias_MTR07_EarthPowerButton.GetReference()
    If powerButton != None
        RegisterForRemoteEvent(powerButton, "OnLoad")
    EndIf
    ObjectReference wheel = Alias_MTR07_EarthWheel.GetReference()
    If wheel != None
        RegisterForRemoteEvent(wheel, "OnLoad")
    EndIf
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender == Game.GetPlayer() && akBaseItem == IgnitionReactorCore01
        CheckCoreInventory()
    EndIf
EndEvent

Event ObjectReference.OnLoad(ObjectReference akSender)
    If akSender == Alias_MTR07_EarthPowerButton.GetReference()
        ; The button resets itself to inactive on every load.
        StartTimer(1.0, 901)
    ElseIf akSender == Alias_MTR07_EarthWheel.GetReference() && IsRunning() && IsStageDone(200) && !IsStageDone(255)
        akSender.PlayAnimation("JumpState02")
    EndIf
EndEvent

Function CheckCoreInventory()
    If !IsRunning() || !IsStageDone(10) || IsStageDone(20) || IsStageDone(70) || IgnitionReactorCore01 == None
        Return
    EndIf
    Int coreCount = Game.GetPlayer().GetItemCount(IgnitionReactorCore01)
    Int coreStage = 30
    While coreStage <= 60
        If IsStageDone(coreStage)
            coreCount += 1
        EndIf
        coreStage += 10
    EndWhile
    If coreCount >= CoresRequired
        SetStage(20)
    EndIf
EndFunction

Function EnsureAttackTimer()
    Quest earthQuest = Self as Quest
    If B21WheelTimerStarted || !earthQuest.IsRunning() || !earthQuest.IsStageDone(200) || earthQuest.IsStageDone(255)
        Return
    EndIf

    GlobalVariable wheelTime = Game.GetFormFromFile(0x003DA814, "SeventySix.esm") as GlobalVariable
    Float duration = 1800.0
    If wheelTime != None && wheelTime.GetValue() > 0.0
        duration = wheelTime.GetValue()
    EndIf
    B21WheelTimerStarted = True
    StartTimer(duration, AttackTimerID)
EndFunction

Function ApplyMachineState()
    If !IsRunning()
        Return
    EndIf
    Bool depleted = IsStageDone(255)
    UpdateCoreSlot(MTR07_EarthIgnitionCoreReactorTrigger01Ref, MTR07_EarthStaticCore01Ref, 30, depleted)
    UpdateCoreSlot(MTR07_EarthIgnitionCoreReactorTrigger02Ref, MTR07_EarthStaticCore02Ref, 40, depleted)
    UpdateCoreSlot(MTR07_EarthIgnitionCoreReactorTrigger03Ref, MTR07_EarthStaticCore03Ref, 50, depleted)
    UpdateCoreSlot(MTR07_EarthIgnitionCoreReactorTrigger04Ref, MTR07_EarthStaticCore04Ref, 60, depleted)
    SetMachineRunning(IsStageDone(200) && !depleted)
    UpdatePowerButton()
EndFunction

Function SetMachineIdle()
    MTR07_EarthIgnitionCoreReactorTrigger01Ref.Disable()
    MTR07_EarthIgnitionCoreReactorTrigger02Ref.Disable()
    MTR07_EarthIgnitionCoreReactorTrigger03Ref.Disable()
    MTR07_EarthIgnitionCoreReactorTrigger04Ref.Disable()
    MTR07_EarthStaticCore01Ref.Disable()
    MTR07_EarthStaticCore02Ref.Disable()
    MTR07_EarthStaticCore03Ref.Disable()
    MTR07_EarthStaticCore04Ref.Disable()
    SetMachineRunning(False)
    ToggleButtonScript powerButton = Alias_MTR07_EarthPowerButton.GetReference() as ToggleButtonScript
    If powerButton != None
        powerButton.SetActiveNoWait(False)
    EndIf
EndFunction

Function UpdateCoreSlot(ObjectReference portTrigger, ObjectReference staticCore, Int installStage, Bool depleted)
    If IsStageDone(installStage) && !depleted
        staticCore.Enable()
    Else
        staticCore.Disable()
    EndIf
    If IsStageDone(10) && !IsStageDone(installStage) && !depleted
        portTrigger.Enable()
    Else
        portTrigger.Disable()
    EndIf
EndFunction

Function SetMachineRunning(Bool running)
    ; The initially disabled reactor toggle is the saved record of whether the machine runs.
    Bool wasRunning = MTR07ReactorsToggle.IsEnabled()
    If running
        MTR07ReactorsToggle.Enable()
        MTR07SoundEnabler.Enable()
        OBJBucketExcavatorEnableMarker.Enable()
    Else
        MTR07ReactorsToggle.Disable()
        MTR07SoundEnabler.Disable()
        OBJBucketExcavatorEnableMarker.Disable()
    EndIf
    ObjectReference wheel = Alias_MTR07_EarthWheel.GetReference()
    If running != wasRunning && wheel != None && wheel.Is3DLoaded()
        If running
            wheel.PlayAnimation("Play01")
        Else
            wheel.PlayAnimation("Play02")
        EndIf
    EndIf
    SetExcavatorArmRunning(MTR07_EarthBucketExcavator_Back01Ref, running)
    SetExcavatorArmRunning(MTR07_EarthBucketExcavator_Front01Ref, running)
EndFunction

Function SetExcavatorArmRunning(ObjectReference arm, Bool running)
    Default2StateActivator twoState = arm as Default2StateActivator
    If twoState != None && twoState.isOpen != running
        twoState.SetOpenNoWait(running)
    EndIf
EndFunction

Function UpdatePowerButton()
    ToggleButtonScript powerButton = Alias_MTR07_EarthPowerButton.GetReference() as ToggleButtonScript
    If powerButton != None
        powerButton.SetActiveNoWait(IsRunning() && IsStageDone(70) && !IsStageDone(200))
    EndIf
EndFunction
