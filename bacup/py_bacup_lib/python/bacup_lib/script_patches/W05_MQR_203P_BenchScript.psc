Event OnActivate(ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If akActionRef != Game.GetPlayer() || GetReference() == None || owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    If owningQuest.IsStageDone(9001) || owningQuest.IsStageDone(8000) || owningQuest.IsStageDone(8100)
        Return
    EndIf
    Int nextStage = 0
    If owningQuest.IsStageDone(1450) && !owningQuest.IsStageDone(1500)
        nextStage = 1500
    ElseIf owningQuest.IsStageDone(1050) && !owningQuest.IsStageDone(1100)
        nextStage = 1100
    ElseIf owningQuest.IsStageDone(600) && !owningQuest.IsStageDone(605)
        nextStage = 605
    EndIf
    If nextStage > 0 && owningQuest.SetStage(nextStage) && W05_MQR_203P_PassTimeSpell != None
        W05_MQR_203P_PassTimeSpell.Cast(akActionRef, akActionRef)
    EndIf
EndEvent
