; Shared air-drop controller for GQ_DropGovtMaster, Nuke_LaunchCardPatrol and RE_SceneKMK02.
; FO76 handed the flight to the SQ_AirDrop child quest, whose SQ_AirDropNode is absent from the
; converted master, so the drop is driven here: pick the drop point, put a vertibot over it,
; release AirDropContainer on arrival (or where the vertibot is shot down) and set the bound stages.
; Timer ids 7610-7628 are a 5 s poll whose id carries the elapsed ticks, so one chain both
; polls and measures the flight without a script variable. 7650 dismisses the vertibot.

Event OnQuestInit()
    BeginAirDrop()
EndEvent

Function BeginAirDrop()
    ObjectReference dropPoint
    If Alias_AirDropPoint != None
        dropPoint = Alias_AirDropPoint.GetReference()
    EndIf
    If dropPoint == None
        dropPoint = FindLocalDropPoint()
        If dropPoint != None && Alias_AirDropPoint != None
            Alias_AirDropPoint.ForceRefTo(dropPoint)
        EndIf
    EndIf
    If dropPoint == None
        Debug.Trace("[SQ_AirDropMasterScript] " + Self + " found no drop point")
        If NoDropTargetStage >= 0
            SetStage(NoDropTargetStage)
        EndIf
        Return
    EndIf
    If DropStartStage >= 0
        SetStage(DropStartStage)
    EndIf
    Actor vertibot = PrepareVertibot(dropPoint)
    Debug.Trace("[SQ_AirDropMasterScript] " + Self + " drop point=" + dropPoint + " vertibot=" + vertibot)
    StartTimer(5.0, 7610)
EndFunction

ObjectReference Function FindLocalDropPoint()
    ; RE_SceneKMK02 binds its encounter centre as the trigger point and drops there.
    ; GQ_DropGovtMaster leaves its radio-station trigger empty and drops at an SQ_AirDropMarker
    ; between SQ_DropGovtTargetDistanceMin and Max from the player.
    If Alias_TriggerPoint != None && Alias_TriggerPoint.GetReference() != None
        Return Alias_TriggerPoint.GetReference()
    EndIf
    Actor player = Game.GetPlayer()
    Float minDistance = 8000.0
    Float maxDistance = 50000.0
    GlobalVariable minGlobal = Game.GetFormFromFile(0x0000EF52, "SeventySix.esm") as GlobalVariable
    GlobalVariable maxGlobal = Game.GetFormFromFile(0x0000EF53, "SeventySix.esm") as GlobalVariable
    If minGlobal != None
        minDistance = minGlobal.GetValue()
    EndIf
    If maxGlobal != None
        maxDistance = maxGlobal.GetValue()
    EndIf
    ; The markers are persistent but FO4 Papyrus can only search loaded cells; Tales scans the
    ; worldspace's persistent references when it is installed.
    Var[] args = new Var[4]
    args[0] = player
    args[1] = minDistance
    args[2] = maxDistance
    args[3] = False
    ObjectReference marker = Utility.CallGlobalFunction("B21:B21_TFA_VertibotNative", "FindAirDropMarker", args) as ObjectReference
    If marker == None
        Form markerBase = Game.GetFormFromFile(0x00017F68, "SeventySix.esm")
        If markerBase != None
            marker = Game.FindClosestReferenceOfTypeFromRef(markerBase, player, maxDistance)
        EndIf
    EndIf
    Return marker
EndFunction

Actor Function AirDropVertibot()
    If Alias_Vertibot == None
        Return None
    EndIf
    Return Alias_Vertibot.GetActorReference()
EndFunction

Actor Function PrepareVertibot(ObjectReference akDropPoint)
    Actor vertibot = AirDropVertibot()
    If vertibot == None
        ActorBase vertibotBase = Game.GetFormFromFile(0x00004049, "SeventySix.esm") as ActorBase
        If vertibotBase == None || Alias_Vertibot == None
            Return None
        EndIf
        vertibot = akDropPoint.PlaceActorAtMe(vertibotBase)
        If vertibot == None
            Return None
        EndIf
        vertibot.SetPosition(akDropPoint.GetPositionX() + 2500.0, akDropPoint.GetPositionY() + 2500.0, akDropPoint.GetPositionZ() + 2500.0)
        Alias_Vertibot.ForceRefTo(vertibot)
    EndIf
    ; The converted orbit packages follow SeventySix's SQLinkOrbit, while the quest aliases link
    ; Fallout 4's RELinkOrbit, so point the SeventySix keyword at the drop point as well.
    Keyword orbitLink = Game.GetFormFromFile(0x001090F5, "SeventySix.esm") as Keyword
    If orbitLink != None
        vertibot.SetLinkedRef(akDropPoint, orbitLink)
    EndIf
    vertibot.SetAllowFlying(True)
    vertibot.EvaluatePackage()
    RegisterForRemoteEvent(vertibot, "OnDeath")
    Return vertibot
EndFunction

Bool Function IsAirDropReleased()
    Return Alias_AirDropContainer != None && Alias_AirDropContainer.GetReference() != None
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7650
        DismissVertibot()
        Return
    EndIf
    If aiTimerID < 7610 || aiTimerID > 7628 || !IsRunning() || IsAirDropReleased()
        Return
    EndIf
    Int tick = aiTimerID - 7610
    ObjectReference dropPoint
    If Alias_AirDropPoint != None
        dropPoint = Alias_AirDropPoint.GetReference()
    EndIf
    If dropPoint == None
        Return
    EndIf
    Actor vertibot = AirDropVertibot()
    If vertibot != None && vertibot.IsDead()
        ; OnDeath releases the cargo at the wreck.
        Return
    EndIf
    If vertibot != None && vertibot.GetDistance(dropPoint) <= 4000.0
        ReleaseAirDrop(dropPoint)
    ElseIf tick >= 9 && (vertibot == None || !vertibot.Is3DLoaded())
        ; After 45 s an unseen vertibot is put over the drop point instead of trusting
        ; unloaded flight.
        If vertibot != None
            vertibot.MoveTo(dropPoint, 0.0, 0.0, 2500.0)
        EndIf
        ReleaseAirDrop(dropPoint)
    ElseIf tick >= 18
        ReleaseAirDrop(dropPoint)
    Else
        StartTimer(5.0, aiTimerID + 1)
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If !IsRunning() || IsAirDropReleased() || akSender != AirDropVertibot()
        Return
    EndIf
    ; FO76 drops the cargo where the vertibot comes down; give the wreck time to land.
    Utility.Wait(4.0)
    ReleaseAirDrop(akSender)
EndEvent

Function ReleaseAirDrop(ObjectReference akAt)
    If akAt == None || AirDropContainer == None || !IsRunning() || IsAirDropReleased()
        Return
    EndIf
    ObjectReference crate = akAt.PlaceAtMe(AirDropContainer)
    If crate == None
        Return
    EndIf
    If IsAirDropReleased()
        crate.Delete()
        Return
    EndIf
    If Alias_AirDropContainer != None
        Alias_AirDropContainer.ForceRefTo(crate)
    EndIf
    If akAt == AirDropVertibot() && crate.Is3DLoaded()
        crate.MoveToNearestNavmeshLocation()
    EndIf
    Debug.Trace("[SQ_AirDropMasterScript] " + Self + " released " + crate + " at " + akAt)
    Nuke_LaunchCardPatrolScript launchCardPatrol = (Self as Quest) as Nuke_LaunchCardPatrolScript
    If launchCardPatrol != None
        launchCardPatrol.WatchLaunchCardCrate(crate)
    EndIf
    GQ_DropGovtScript governmentDrop = (Self as Quest) as GQ_DropGovtScript
    If governmentDrop != None
        governmentDrop.WatchSupplyChest(crate)
    EndIf
    If DropCompleteStage >= 0
        SetStage(DropCompleteStage)
    EndIf
    StartTimer(300.0, 7650)
EndFunction

Function DismissVertibot()
    Actor vertibot = AirDropVertibot()
    If vertibot == None || vertibot.IsDead()
        Return
    EndIf
    If vertibot.Is3DLoaded()
        StartTimer(60.0, 7650)
    Else
        vertibot.Delete()
    EndIf
EndFunction

Event OnQuestShutdown()
    Actor vertibot = AirDropVertibot()
    If vertibot != None && !vertibot.IsDead() && !vertibot.Is3DLoaded()
        vertibot.Delete()
    EndIf
EndEvent
