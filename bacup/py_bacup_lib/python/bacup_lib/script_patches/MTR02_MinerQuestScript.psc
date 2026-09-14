Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == GetSchematicsStage
        GrantExcavatorArmorForLocalProgress()
        MTR02_MinerAliasScript playerAliasScript = MTR02_MinerPlayer as MTR02_MinerAliasScript
        If playerAliasScript != None
            playerAliasScript.ReconcileEquippedArmor()
        EndIf
    ElseIf auiStageID >= 40 && auiStageID <= 90
        CheckArmorProgress()
    EndIf
EndEvent

Function GrantExcavatorArmorForLocalProgress()
    Actor player = Game.GetPlayer()
    If player == None || !IsRunning() || GetStage() < GetSchematicsStage || IsStageDone(255)
        Return
    EndIf

    If Armor_Power_Excavator_ArmLeft != None && player.GetItemCount(Armor_Power_Excavator_ArmLeft) == 0
        player.AddItem(Armor_Power_Excavator_ArmLeft, 1, False)
    EndIf
    If Armor_Power_Excavator_ArmRight != None && player.GetItemCount(Armor_Power_Excavator_ArmRight) == 0
        player.AddItem(Armor_Power_Excavator_ArmRight, 1, False)
    EndIf
    If Armor_Power_Excavator_Helmet != None && player.GetItemCount(Armor_Power_Excavator_Helmet) == 0
        player.AddItem(Armor_Power_Excavator_Helmet, 1, False)
    EndIf
    If Armor_Power_Excavator_Torso != None && player.GetItemCount(Armor_Power_Excavator_Torso) == 0
        player.AddItem(Armor_Power_Excavator_Torso, 1, False)
    EndIf
    If Armor_Power_Excavator_LegLeft != None && player.GetItemCount(Armor_Power_Excavator_LegLeft) == 0
        player.AddItem(Armor_Power_Excavator_LegLeft, 1, False)
    EndIf
    If Armor_Power_Excavator_LegRight != None && player.GetItemCount(Armor_Power_Excavator_LegRight) == 0
        player.AddItem(Armor_Power_Excavator_LegRight, 1, False)
    EndIf
EndFunction

Function CheckArmorProgress()
    Bool armsComplete = IsStageDone(40) && IsStageDone(50)
    Bool legsComplete = IsStageDone(80) && IsStageDone(90)
    Bool armorComplete = armsComplete && legsComplete && IsStageDone(60) && IsStageDone(70)

    If armsComplete && !IsStageDone(100)
        SetStage(100)
    EndIf
    If legsComplete && !IsStageDone(110)
        SetStage(110)
    EndIf
    If armorComplete && !IsStageDone(120)
        SetStage(120)
    EndIf
EndFunction
