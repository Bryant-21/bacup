Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && W05_MQR_203P_PassTimeSpell != None
        W05_MQR_203P_PassTimeSpell.Cast(akActionRef, akActionRef)

        Quest owningQuest = GetOwningQuest()
        If owningQuest != None
            Int currentStage = owningQuest.GetStage()
            If currentStage == 600 && !owningQuest.IsStageDone(605)
                owningQuest.SetStage(605)
            ElseIf currentStage == 1050 && !owningQuest.IsStageDone(1100)
                owningQuest.SetStage(1100)
            ElseIf currentStage == 1450 && !owningQuest.IsStageDone(1500)
                owningQuest.SetStage(1500)
            EndIf
        EndIf
    EndIf
EndEvent
