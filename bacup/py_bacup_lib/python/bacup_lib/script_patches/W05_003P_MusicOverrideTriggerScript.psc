Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(ScorchedCombatStage) && !owningQuest.IsStageDone(800)
        W05_003P_Muscle_QuestScript controller = owningQuest as W05_003P_Muscle_QuestScript
        If controller
            controller.StartLocalCombatMusic()
        EndIf
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    W05_003P_Muscle_QuestScript controller = GetOwningQuest() as W05_003P_Muscle_QuestScript
    If controller
        controller.StopLocalCombatMusic()
    EndIf
EndEvent
