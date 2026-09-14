Scriptname B21:PlanLearnOnRead Extends ObjectReference

GlobalVariable Property Learned Auto Const
Quest Property ReadQuest Auto Const
Int Property PrereqStage = -1 Auto Const
Int Property StageToSet = -1 Auto Const

Bool reading = False

Event OnRead()
    If reading
        Return
    EndIf
    reading = True
    LearnPlan()
    reading = False
EndEvent

Function LearnPlan()
    Actor playerRef = Game.GetPlayer()
    If GetContainer() != playerRef
        Return
    EndIf
    If Learned == None
        Debug.Notification("This plan has no available recipe to learn.")
        Return
    EndIf
    If ReadQuest != None && !ReadQuest.IsStageDone(StageToSet)
        If !ReadQuest.IsRunning() || ReadQuest.IsCompleted()
            Debug.Notification("Continue the related quest before learning this plan.")
            Return
        EndIf
        If PrereqStage >= 0 && !ReadQuest.IsStageDone(PrereqStage)
            Debug.Notification("Continue the related quest before learning this plan.")
            Return
        EndIf
        ReadQuest.SetStage(StageToSet)
        If !ReadQuest.IsStageDone(StageToSet)
            Debug.Notification("Unable to learn this plan. The plan has been kept.")
            Return
        EndIf
    EndIf
    If Learned.GetValue() >= 1.0
        Debug.Notification("You already know this plan.")
        Return
    EndIf
    Learned.SetValue(1.0)
    If Learned.GetValue() >= 1.0
        playerRef.RemoveItem(Self, 1, True)
        Debug.Notification("Plan learned.")
    EndIf
EndFunction
