Actor Function EN05Basic_GetPlayer()
    Actor player = None
    If Alias_ActivePlayer != None
        player = Alias_ActivePlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

Function EN05Basic_RecordStage(Int aiStage)
    Actor player = EN05Basic_GetPlayer()
    If player != None && EN05_StageValue != None
        player.SetValue(EN05_StageValue, aiStage as Float)
    EndIf
EndFunction

Function EN05Basic_TallyCourses()
    Int completed = 0
    If IsStageDone(30)
        completed += 1
    EndIf
    If IsStageDone(40)
        completed += 1
    EndIf
    If IsStageDone(50)
        completed += 1
    EndIf

    EN05_QuestScript basic = (Self as Quest) as EN05_QuestScript
    If basic != None
        basic.iCoursesCompletedCount = completed
    EndIf

    If completed >= 3 && !IsStageDone(90)
        SetObjectiveDisplayed(90)
        SetStage(90)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor player = EN05Basic_GetPlayer()
    If player != None && EN05_Basic_QuestStarted != None
        player.SetValue(EN05_Basic_QuestStarted, 1.0)
    EndIf
    SetObjectiveDisplayed(4)
    EN05Basic_RecordStage(4)
EndFunction

Function Fragment_Stage_0005_Item_00()
    Actor player = EN05Basic_GetPlayer()
    If player != None && EN05_Basic_QuestStarted != None
        player.SetValue(EN05_Basic_QuestStarted, 1.0)
    EndIf
    SetObjectiveCompleted(4)
    SetObjectiveDisplayed(5)
    EN05Basic_RecordStage(5)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(5)
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(11)
    SetObjectiveDisplayed(12)
    EN05Basic_RecordStage(10)
EndFunction

Function Fragment_Stage_0011_Item_00()
    Actor player = EN05Basic_GetPlayer()
    If player != None && EN05_PlayerReadUniformLogValue != None
        player.SetValue(EN05_PlayerReadUniformLogValue, 1.0)
    EndIf
    SetObjectiveCompleted(12)
    SetObjectiveDisplayed(13)
    EN05Basic_RecordStage(11)
EndFunction

Function Fragment_Stage_0012_Item_00()
    SetObjectiveCompleted(12)
    SetObjectiveCompleted(13)

    Actor player = EN05Basic_GetPlayer()
    If player != None && Alias_PlayerCanCollectUniform != None
        Alias_PlayerCanCollectUniform.ForceRefTo(player)
    EndIf

    SetObjectiveDisplayed(15)
    EN05Basic_RecordStage(12)
EndFunction

Function Fragment_Stage_0014_Item_00()
EndFunction

Function Fragment_Stage_0015_Item_00()
    Actor player = EN05Basic_GetPlayer()
    If player != None
        If ClothesFatiguesPostWar != None && !player.WornHasKeyword(EN05_MilitaryFatigueKeyword) \
            && player.GetItemCount(ClothesFatiguesPostWar) < 1
            player.AddItem(ClothesFatiguesPostWar, 1, True)
        EndIf
        If Armor_Army_Helmet_postwar != None && !player.WornHasKeyword(EN05_MilitaryHelmetKeyword) \
            && player.GetItemCount(Armor_Army_Helmet_postwar) < 1
            player.AddItem(Armor_Army_Helmet_postwar, 1, True)
        EndIf
        If EN05_UniformVoucher != None && player.GetItemCount(EN05_UniformVoucher) > 0
            player.RemoveItem(EN05_UniformVoucher, player.GetItemCount(EN05_UniformVoucher), True)
        EndIf
    EndIf

    SetObjectiveCompleted(12)
    SetObjectiveCompleted(13)
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(10)
    SetObjectiveDisplayed(11)
    EN05Basic_RecordStage(15)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveSkipped(12)
    SetObjectiveSkipped(13)
    SetObjectiveSkipped(15)
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(11)

    If Alias_PlayerCanCollectUniform != None
        Alias_PlayerCanCollectUniform.Clear()
    EndIf

    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
    SetObjectiveDisplayed(50)
    EN05Basic_RecordStage(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(30)
    EN05Basic_RecordStage(30)
    EN05Basic_TallyCourses()
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(40)
    EN05Basic_RecordStage(40)
    EN05Basic_TallyCourses()
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(50)
    EN05Basic_RecordStage(50)
    EN05Basic_TallyCourses()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(90)

    Actor player = EN05Basic_GetPlayer()
    If player != None && Alias_PlayerReadyForCombat != None
        Alias_PlayerReadyForCombat.ForceRefTo(player)
    EndIf

    SetObjectiveDisplayed(100)
    EN05Basic_RecordStage(100)
EndFunction

Function Fragment_Stage_0135_Item_00()
    SetObjectiveCompleted(100)
    EN05Basic_RecordStage(135)
    If !IsStageDone(140)
        SetStage(140)
    EndIf
EndFunction

Function Fragment_Stage_0140_Item_00()
    SetObjectiveCompleted(100)

    If Alias_PlayerReadyForCombat != None
        Alias_PlayerReadyForCombat.Clear()
    EndIf

    SetObjectiveDisplayed(140)
    EN05Basic_RecordStage(140)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(140)
    CompleteAllObjectives()

    Actor player = EN05Basic_GetPlayer()
    If player != None && EN05_CompletedValue != None
        player.SetValue(EN05_CompletedValue, 1.0)
    EndIf

    EN05Basic_RecordStage(150)

    If player != None && EN05_BackToMODUSMiscQuestStartKeyword != None
        EN05_BackToMODUSMiscQuestStartKeyword.SendStoryEvent(None, player, player)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    EN05Basic_RecordStage(200)
    Stop()
EndFunction

Function Fragment_Stage_0201_Item_00()
    If Alias_PlayerCanCollectUniform != None
        Alias_PlayerCanCollectUniform.Clear()
    EndIf
    If Alias_PlayerReadyForCombat != None
        Alias_PlayerReadyForCombat.Clear()
    EndIf
EndFunction
