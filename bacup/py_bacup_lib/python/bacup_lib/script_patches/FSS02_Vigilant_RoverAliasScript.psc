Quest Function OwningQuest()
    If MyQuest == None
        MyQuest = GetOwningQuest()
    EndIf
    Return MyQuest
EndFunction

Function StartRepairWindow()
    Float window = 60.0
    If FSS02_Vigilant_RepairTimer != None && FSS02_Vigilant_RepairTimer.GetValue() > 0.0
        window = FSS02_Vigilant_RepairTimer.GetValue()
    EndIf
    CancelTimer(1)
    StartTimer(window, 1)
EndFunction

Function CancelRepairWindow()
    CancelTimer(1)
EndFunction

Function StartScanningFX()
    Actor rover = GetActorReference()
    If rover == None
        Return
    EndIf
    FSS02_Vigilant_RoverActorScript roverActor = rover as FSS02_Vigilant_RoverActorScript
    If roverActor == None
        Return
    EndIf

    ObjectReference aimTarget = None
    If FSS02_Vigilant_AimMarkerList != None
        ObjectReference[] aimMarkers = rover.FindAllReferencesOfType(FSS02_Vigilant_AimMarkerList as Form, 4096.0)
        If aimMarkers != None && aimMarkers.Length > 0
            aimTarget = aimMarkers[Utility.RandomInt(0, aimMarkers.Length - 1)]
        EndIf
    EndIf
    roverActor.PlayScanningFX(aimTarget)
EndFunction

Function StopScanningFX()
    Actor rover = GetActorReference()
    If rover == None
        Return
    EndIf
    FSS02_Vigilant_RoverActorScript roverActor = rover as FSS02_Vigilant_RoverActorScript
    If roverActor != None
        roverActor.StopScanningFX()
    EndIf
EndFunction

Function BeginPodReturn()
    CancelTimer(1)
    StartTimer(5.0, 2)
EndFunction

Event OnAliasInit()
    MyQuest = GetOwningQuest()
EndEvent

Bool Function RepairIsOpen()
    Quest owner = OwningQuest()
    If owner == None || GetActorReference() == None || !owner.IsRunning()
        Return False
    EndIf
    If !owner.IsStageDone(Stage_RoverSetUpDone)
        Return False
    EndIf
    Return !owner.IsStageDone(Stage_RoverRepairDone) && !owner.IsStageDone(Stage_QuestTimerExpired) && !owner.IsStageDone(Stage_RepairTimerExpired)
EndFunction

Function ArmRepairApproachPoll()
    CancelTimer(3)
    StartTimer(3.0, 3)
EndFunction

Function RepairRover()
    Quest owner = OwningQuest()
    Actor rover = GetActorReference()
    If owner == None || rover == None || !RepairIsOpen()
        Return
    EndIf

    CancelTimer(3)
    CancelRepairWindow()
    If rover.IsBleedingOut() || rover.GetValuePercentage(Game.GetHealthAV()) < 1.0
        rover.ResetHealthAndLimbs()
    EndIf
    If FSS02_Vigilant_RoverBleedoutScene != None && FSS02_Vigilant_RoverBleedoutScene.IsPlaying()
        FSS02_Vigilant_RoverBleedoutScene.Stop()
    EndIf
    owner.SetObjectiveCompleted(Obj_RepairRover)
    If FSS02_Vigilant_RoverFixScene != None && !FSS02_Vigilant_RoverFixScene.IsPlaying()
        FSS02_Vigilant_RoverFixScene.Start()
    EndIf
    StartScanningFX()

    If !owner.IsStageDone(Stage_PlayerInitiated)
        owner.SetStage(Stage_PlayerInitiated)
    Else
        owner.SetObjectiveDisplayed(Obj_ProtectRover, True, True)
    EndIf
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RepairRover()
    EndIf
EndEvent

Event OnEnterBleedout()
    Quest owner = OwningQuest()
    If owner == None || !owner.IsRunning()
        Return
    EndIf
    If !owner.IsStageDone(Stage_PlayerInitiated) || owner.IsStageDone(Stage_RoverRepairDone)
        Return
    EndIf

    StopScanningFX()
    If FSS02_Vigilant_RoverFixScene != None && FSS02_Vigilant_RoverFixScene.IsPlaying()
        FSS02_Vigilant_RoverFixScene.Stop()
    EndIf
    If FSS02_Vigilant_RoverBleedoutScene != None && !FSS02_Vigilant_RoverBleedoutScene.IsPlaying()
        FSS02_Vigilant_RoverBleedoutScene.Start()
    EndIf
    owner.SetObjectiveCompleted(Obj_RepairRover, False)
    owner.SetObjectiveDisplayed(Obj_RepairRover, True, True)
    StartRepairWindow()
    ArmRepairApproachPoll()
EndEvent

Event OnTimer(Int aiTimerID)
    Quest owner = OwningQuest()
    If owner == None || !owner.IsRunning()
        Return
    EndIf

    If aiTimerID == 1
        Actor rover = GetActorReference()
        If owner.IsStageDone(Stage_RoverRepairDone) || owner.IsObjectiveCompleted(Obj_RepairRover)
            Return
        EndIf
        If rover != None && !rover.IsBleedingOut()
            Return
        EndIf
        If !owner.IsStageDone(Stage_RepairTimerExpired)
            owner.SetStage(Stage_RepairTimerExpired)
        EndIf
    ElseIf aiTimerID == 2
        Actor rover = GetActorReference()
        ObjectReference pod = None
        If Alias_RoverPod != None
            pod = Alias_RoverPod.GetReference()
        EndIf
        If rover == None || pod == None || rover.GetDistance(pod) <= 256.0
            FSS02_Vigilant_QuestScript eventScript = owner as FSS02_Vigilant_QuestScript
            If eventScript != None
                eventScript.ShutdownEvent()
            EndIf
            Return
        EndIf
        StartTimer(5.0, 2)
    ElseIf aiTimerID == 3
        ; FO4 gives a robot with no dialogue no activation prompt, so closing on
        ; Rover stands in for FO76's hold-to-repair interaction.
        Actor rover = GetActorReference()
        Actor playerRef = Game.GetPlayer()
        If !RepairIsOpen() || owner.IsObjectiveCompleted(Obj_RepairRover)
            Return
        EndIf
        If rover != None && playerRef != None && rover.Is3DLoaded() && rover.GetDistance(playerRef) <= 192.0
            RepairRover()
            Return
        EndIf
        StartTimer(3.0, 3)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
    CancelTimer(2)
    CancelTimer(3)
EndEvent
