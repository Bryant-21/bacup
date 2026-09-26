Function RegisterPodTrigger()
    ObjectReference triggerRef = GetLinkedRef(LinkTrigger)
    If triggerRef != None
        RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
        RegisterForRemoteEvent(triggerRef, "OnTriggerLeave")
    EndIf
EndFunction

Event OnInit()
    RegisterPodTrigger()
EndEvent

Event OnLoad()
    B21PlayerInsidePod = False
    B21PodTravelBusy = False
    RegisterPodTrigger()
EndEvent

Event OnUnload()
    UnregisterForAllRemoteEvents()
    B21PlayerInsidePod = False
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akSender == GetLinkedRef(LinkTrigger) && akActionRef == Game.GetPlayer()
        B21PlayerInsidePod = True
    EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
    If akSender == GetLinkedRef(LinkTrigger) && akActionRef == Game.GetPlayer()
        B21PlayerInsidePod = False
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || !B21PlayerInsidePod || B21PodTravelBusy
        Return
    EndIf
    If MoMRank == None || playerRef.GetValue(MoMRank) < 1.0
        If MoMCryptosVoiceF_VOICEONLY != None && MoMMaster_SecretExitFailureTopic != None
            MoMCryptosVoiceF_VOICEONLY.Say(MoMMaster_SecretExitFailureTopic, None, False, playerRef)
        EndIf
        Return
    EndIf
    ObjectReference doorRef = GetLinkedRef(LinkDoor)
    ObjectReference partnerPod = GetLinkedRef(LC022LinkPod)
    If doorRef == None || partnerPod == None || partnerPod.GetLinkedRef(LinkDoor) == None
        Return
    EndIf
    B21PodTravelBusy = True
    If doorRef.Activate(playerRef)
        B21PlayerInsidePod = False
    EndIf
    B21PodTravelBusy = False
EndEvent
