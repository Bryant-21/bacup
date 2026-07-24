; TODO

Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    W05_MQR_205P_QuestScript owningQuestScript = GetOwningQuest() as W05_MQR_205P_QuestScript
    If owningQuestScript == None || owningQuestScript.RaRaCowerIdleMarker == None
        Return
    EndIf

    ObjectReference triggerRef = GetReference()
    ObjectReference cowerMarker = owningQuestScript.RaRaCowerIdleMarker.GetReference()
    Actor raRaRef = owningQuestScript.RaRa.GetActorReference()
    If triggerRef != None && cowerMarker != None
        cowerMarker.MoveTo(triggerRef)
    EndIf
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndEvent
