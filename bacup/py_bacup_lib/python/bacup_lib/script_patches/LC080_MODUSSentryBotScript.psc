Function RegisterSentryTriggers()
    If IsDead()
        Return
    EndIf
    If LC080_LinkSetConsciousTrigger != None
        ObjectReference wakeTrigger = GetLinkedRef(LC080_LinkSetConsciousTrigger)
        If wakeTrigger != None
            RegisterForRemoteEvent(wakeTrigger, "OnTriggerEnter")
        EndIf
    EndIf
    If LC080_LinkSetUnconsciousTrigger != None
        ObjectReference sleepTrigger = GetLinkedRef(LC080_LinkSetUnconsciousTrigger)
        If sleepTrigger != None
            RegisterForRemoteEvent(sleepTrigger, "OnTriggerEnter")
        EndIf
    EndIf
EndFunction

Event OnInit()
    RegisterSentryTriggers()
EndEvent

Event OnLoad()
    RegisterSentryTriggers()
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || IsDead() || akSender == None
        Return
    EndIf
    If LC080_LinkSetConsciousTrigger != None && akSender == GetLinkedRef(LC080_LinkSetConsciousTrigger)
        SetUnconscious(False)
        EvaluatePackage()
    ElseIf LC080_LinkSetUnconsciousTrigger != None && akSender == GetLinkedRef(LC080_LinkSetUnconsciousTrigger)
        SetUnconscious(True)
        EvaluatePackage()
    EndIf
EndEvent

Event OnUnload()
    UnregisterForAllRemoteEvents()
EndEvent

Event OnDeath(Actor akKiller)
    UnregisterForAllRemoteEvents()
EndEvent
