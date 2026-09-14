Function InitializeLocalState()
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:AC_MQ02_Stage:QuestScript
    SelfRef = GetReference()
EndFunction

Bool Function AllRequiredEquipmentEquipped()
    Actor player = SelfRef as Actor
    If player == None
        Return False
    EndIf
    Int index = 0
    While index < RequiredEquipment.Length
        Form requiredItem = RequiredEquipment[index].ItemToEquip
        If requiredItem != None && !player.IsEquipped(requiredItem)
            Return False
        EndIf
        index += 1
    EndWhile
    Return True
EndFunction

Function RefreshEquipmentObjectives()
    If OwningQuest == None || SelfRef == None
        InitializeLocalState()
    EndIf
    Actor player = SelfRef as Actor
    If player == None || !OwningQuest.IsStageDone(1300) || OwningQuest.IsStageDone(Stage_ClownImpersonationStarted) || OwningQuest.IsStageDone(Stage_ClownImpersonationEnded)
        Return
    EndIf
    Int index = 0
    While index < RequiredEquipment.Length
        ClownDatum required = RequiredEquipment[index]
        OwningQuest.SetObjectiveCompleted(required.Obj_EquipItem, player.IsEquipped(required.ItemToEquip))
        index += 1
    EndWhile
    If AllRequiredEquipmentEquipped() && !OwningQuest.IsStageDone(Stage_ClownImpersonationStarted)
        OwningQuest.SetStage(Stage_ClownImpersonationStarted)
    EndIf
EndFunction

Event OnAliasInit()
    InitializeLocalState()
    RefreshEquipmentObjectives()
EndEvent

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    RefreshEquipmentObjectives()
EndEvent

Event OnItemUnequipped(Form akBaseObject, ObjectReference akReference)
    RefreshEquipmentObjectives()
EndEvent
