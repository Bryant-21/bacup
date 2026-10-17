Function Fragment_Stage_0100_Item_00()
    PublishGatherProgress()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    RecordGatheredItem()
EndFunction

Function Fragment_Stage_0350_Item_00()
    RecordGatheredItem()
EndFunction

Function Fragment_Stage_0450_Item_00()
    RecordGatheredItem()
EndFunction

Function Fragment_Stage_0500_Item_00()
    PublishGatherProgress()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    HandOverPersonalEffect(Item_PE1)
EndFunction

Function Fragment_Stage_0610_Item_00()
    HandOverPersonalEffect(Item_PE2)
EndFunction

Function Fragment_Stage_0620_Item_00()
    HandOverPersonalEffect(Item_PE3)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(20, True)
EndFunction

Function Fragment_Stage_9500_Item_00()
    Stop()
EndFunction

Int Function GatheredItemCount()
    Int gathered = 0
    If IsStageDone(250)
        gathered += 1
    EndIf
    If IsStageDone(350)
        gathered += 1
    EndIf
    If IsStageDone(450)
        gathered += 1
    EndIf
    Return gathered
EndFunction

Function PublishGatherProgress()
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables == None
        Return
    EndIf
    Int target = 3
    DefaultCounterQuestA counter = owner as DefaultCounterQuestA
    If counter != None && counter.TargetValue > 0
        target = counter.TargetValue
    EndIf
    variables.SetVariable("GatherCurrent", GatheredItemCount() as Float)
    variables.SetVariable("GatherMax", target as Float)
EndFunction

Function RecordGatheredItem()
    PublishGatherProgress()
    DefaultCounterQuestA counter = (Self as Quest) as DefaultCounterQuestA
    If counter != None
        counter.Increment()
    ElseIf GatheredItemCount() >= 3 && !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

; Craig's completion line (INFO 7613BE) requires AV_MQ02_PE >= 3, one per returned item.
Function HandOverPersonalEffect(Form akItem)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None
        Return
    EndIf
    If akItem != None && playerRef.GetItemCount(akItem) > 0
        playerRef.RemoveItem(akItem, 1)
    EndIf
    If AV_MQ02_PE != None
        playerRef.ModValue(AV_MQ02_PE, 1.0)
    EndIf
EndFunction
