Function Fragment_Stage_0010_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(200, true, true)
EndFunction

Function Fragment_Stage_0020_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(200, true, true)
EndFunction

Function Fragment_Stage_0030_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(200, true, true)
EndFunction

Function Fragment_Stage_0040_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(200, true, true)
EndFunction

Function Fragment_Stage_0050_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(200, true, true)
EndFunction

Function Fragment_Stage_0100_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(100)
    owningQuest.SetObjectiveDisplayed(200)

    If TadpoleQuest.IsRunning() && !TadpoleQuest.IsCompleted()
        If D01C_Tidy_Quest_Intro != None && !D01C_Tidy_Quest_Intro.IsPlaying()
            D01C_Tidy_Quest_Intro.Start()
        EndIf
    ElseIf D01C_TidyQuest_IntroShort != None && !D01C_TidyQuest_IntroShort.IsPlaying()
        D01C_TidyQuest_IntroShort.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(200)
    owningQuest.SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0900_Item_00()
    D01C_OperationTidyScript tidyQuest = D01C_OperationTidy as D01C_OperationTidyScript
    If tidyQuest != None
        tidyQuest.CleanupWasteDispensers()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        Int wasteToRemove = 5
        Int wasteIndex = 0
        While wasteIndex < D01C_ToxicWaste.Length && wasteToRemove > 0
            Int itemCount = playerRef.GetItemCount(D01C_ToxicWaste[wasteIndex])
            If itemCount > wasteToRemove
                itemCount = wasteToRemove
            EndIf
            If itemCount > 0
                playerRef.RemoveItem(D01C_ToxicWaste[wasteIndex], itemCount, true)
                wasteToRemove -= itemCount
            EndIf
            wasteIndex += 1
        EndWhile
        If D01C_Tidy_DailyTimestamp != None
            playerRef.SetValue(D01C_Tidy_DailyTimestamp, Utility.GetCurrentGameTime())
        EndIf
    EndIf

    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(300)

    D01C_OperationTidyScript tidyQuest = D01C_OperationTidy as D01C_OperationTidyScript
    If tidyQuest != None
        tidyQuest.CleanupWasteDispensers()
    EndIf

    P01C_TadpoleQuest tadpole = TadpoleQuest as P01C_TadpoleQuest
    If tadpole != None
        tadpole.OnActivityCompleted(0)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    D01C_OperationTidyScript tidyQuest = D01C_OperationTidy as D01C_OperationTidyScript
    If tidyQuest != None
        tidyQuest.CleanupWasteDispensers()
    EndIf

EndFunction
