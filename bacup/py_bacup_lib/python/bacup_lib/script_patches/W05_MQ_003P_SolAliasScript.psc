Event OnCripple(ActorValue akActorValue, Bool abCrippled)
    If akActorValue != LimbConditionValue || !abCrippled
        Return
    EndIf
    If W05_MQ_003P_Muscle_BlockSolLimbDamage.GetValueInt() != 0
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest && !owningQuest.IsStageDone(ShutoffStage)
        owningQuest.SetStage(TriggerStage)
        W05_MQ_003P_Muscle_BlockSolLimbDamage.SetValueInt(1)
    EndIf
EndEvent
