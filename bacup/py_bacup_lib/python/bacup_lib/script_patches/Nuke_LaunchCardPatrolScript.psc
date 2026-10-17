; Nuke_LaunchCardPatrol: SQ_AirDropMasterScript on the same quest flies the vertibot to
; TargetMarker and calls WatchLaunchCardCrate when the crate is down. FO76 started the patrol
; from its region manager; Tales and the tracking terminal call RequestLocalPatrol instead.

Bool Function RequestLocalPatrol()
    If IsRunning()
        Return True
    EndIf
    Keyword startKeyword = Game.GetFormFromFile(0x0016AF03, "SeventySix.esm") as Keyword
    Location appalachia = Game.GetFormFromFile(0x0001558C, "SeventySix.esm") as Location
    If startKeyword == None || appalachia == None
        Return False
    EndIf
    If IsCompleted() || IsStageDone(StartCountdownStage)
        Reset()
    EndIf
    Bool sent = startKeyword.SendStoryEventAndWait(appalachia)
    Debug.Trace("[Nuke_LaunchCardPatrolScript] patrol requested sent=" + sent + " running=" + IsRunning())
    Return IsRunning()
EndFunction

ObjectReference Function CurrentLaunchCardHolder()
    If !IsRunning()
        Return None
    EndIf
    ObjectReference crateRef = LaunchCardCrate.GetReference()
    If crateRef != None
        Return crateRef
    EndIf
    Return Vertibot.GetReference()
EndFunction

Event OnQuestInit()
    ; Vertibot's alias package orbits its SQLinkOrbit02 target, which the quest links to the
    ; exit marker. Aim it at the drop target until the crate is down.
    Actor vertibotRef = Vertibot.GetActorReference()
    ObjectReference target = TargetMarker.GetReference()
    If vertibotRef != None && target != None && SQLinkOrbit02 != None
        vertibotRef.SetLinkedRef(target, SQLinkOrbit02)
        vertibotRef.EvaluatePackage()
    EndIf
    ObjectReference crateRef = LaunchCardCrate.GetReference()
    If crateRef != None
        WatchLaunchCardCrate(crateRef)
    EndIf
EndEvent

Function WatchLaunchCardCrate(ObjectReference akCrate)
    If akCrate == None
        Return
    EndIf
    If LaunchCardCrate.GetReference() != akCrate
        LaunchCardCrate.ForceRefTo(akCrate)
    EndIf
    ; The container's own leveled list already carries the card; never add a second one.
    If Nuke_LaunchCard != None && akCrate.GetItemCount(Nuke_LaunchCard) == 0
        akCrate.AddItem(Nuke_LaunchCard, 1, True)
    EndIf
    RegisterForRemoteEvent(akCrate, "OnOpen")
    RetargetLaunchCardHunt(akCrate)

    Actor vertibotRef = Vertibot.GetActorReference()
    ObjectReference exitMarker = VertibirdEndMarker.GetReference()
    If vertibotRef != None && !vertibotRef.IsDead() && exitMarker != None && SQLinkOrbit02 != None
        vertibotRef.SetLinkedRef(exitMarker, SQLinkOrbit02)
        vertibotRef.EvaluatePackage()
    EndIf
    StartTimer(ShutdownCheckSeconds, shutdownTimerID)
EndFunction

Function RetargetLaunchCardHunt(ObjectReference akCrate)
    ; A running tracking-terminal hunt follows the vertibot; move it onto the crate.
    Quest codeHunt = Game.GetFormFromFile(0x002D0F6A, "SeventySix.esm") as Quest
    If codeHunt == None || !codeHunt.IsRunning()
        Return
    EndIf
    ReferenceAlias huntTarget = codeHunt.GetAlias(0) as ReferenceAlias
    ReferenceAlias huntTarget02 = codeHunt.GetAlias(7) as ReferenceAlias
    ObjectReference vertibotRef = Vertibot.GetReference()
    If huntTarget != None && vertibotRef != None && huntTarget.GetReference() == vertibotRef
        huntTarget.ForceRefTo(akCrate)
        If huntTarget02 != None
            huntTarget02.ForceRefTo(akCrate)
        EndIf
    EndIf
EndFunction

Event ObjectReference.OnOpen(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && !IsStageDone(StartCountdownStage)
        SetStage(StartCountdownStage)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != shutdownTimerID || !IsRunning() || IsStageDone(StartCountdownStage)
        Return
    EndIf
    ; An unopened crate nobody is near or tracking ends the patrol so another can start elsewhere.
    ObjectReference crateRef = LaunchCardCrate.GetReference()
    Quest codeHunt = Game.GetFormFromFile(0x002D0F6A, "SeventySix.esm") as Quest
    Bool tracked = codeHunt != None && codeHunt.IsRunning()
    If crateRef != None && !crateRef.Is3DLoaded() && !tracked
        Debug.Trace("[Nuke_LaunchCardPatrolScript] unopened launch-card crate released")
        Stop()
    Else
        StartTimer(ShutdownCheckSeconds, shutdownTimerID)
    EndIf
EndEvent
