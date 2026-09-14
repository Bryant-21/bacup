Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    If !bObstaclesActive || ActiveTargets == None || akSenderRef == None
        Return
    EndIf
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If ActiveTargets.Find(akSenderRef) < 0
        Return
    EndIf

    ActiveTargets.RemoveRef(akSenderRef)
    If ActiveTargets.GetCount() > 0
        Return
    EndIf

    bObstaclesActive = False
    EN05_ObstacleCourseQuestScript course = GetOwningQuest() as EN05_ObstacleCourseQuestScript
    If course != None && course.IsRunning()
        course.EN05OB_AdvanceTargetSet()
    EndIf
EndEvent
