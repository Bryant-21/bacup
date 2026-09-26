Function ReconcileLocalEquipment()
    MoMItemEquipStateNeedsUpdate = True
    If lock_MoMItemEquipState
        Return
    EndIf
    lock_MoMItemEquipState = True
    While MoMItemEquipStateNeedsUpdate
        MoMItemEquipStateNeedsUpdate = False
        Actor playerRef = Game.GetPlayer()
        MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
        If playerRef == None || masterScript == None || !masterScript.IsRunning()
            lock_MoMItemEquipState = False
            Return
        EndIf
        Bool eyeWasEquippedLast = lastArmorEquipEventWasFromEye
        lastArmorEquipEventWasFromEye = False
        EnforceEyeOfRaRequiresGarb(playerRef, eyeWasEquippedLast)
        Int index = 0
        If MoMEquippableItemList != None
            While index < MoMEquippableItemList.Length
                Form item = MoMEquippableItemList[index].MoMEquippableBaseObject
                ActorValue checkpoint = MoMEquippableItemList[index].MoMEquippableCheckpointValue
                If item != None && checkpoint != None
                    If playerRef.IsEquipped(item)
                        playerRef.SetValue(checkpoint, 1.0)
                    Else
                        playerRef.SetValue(checkpoint, 0.0)
                    EndIf
                EndIf
                index += 1
            EndWhile
        EndIf
        Bool wearingVeil = False
        If MoM_ClothesMistressOfMysteryWornVeil != None
            wearingVeil = playerRef.IsEquipped(MoM_ClothesMistressOfMysteryWornVeil)
        EndIf
        If !wearingVeil && MoM_ClothesMistressOfMysteryVeil != None
            wearingVeil = playerRef.IsEquipped(MoM_ClothesMistressOfMysteryVeil)
        EndIf
        If MoMMistressOfMysteryFaction != None
            If wearingVeil && !playerRef.IsInFaction(MoMMistressOfMysteryFaction)
                playerRef.AddToFaction(MoMMistressOfMysteryFaction)
            ElseIf !wearingVeil && playerRef.IsInFaction(MoMMistressOfMysteryFaction)
                playerRef.RemoveFromFaction(MoMMistressOfMysteryFaction)
            EndIf
        EndIf
        If masterScript.MoMVeilIsEquipped != None
            If wearingVeil
                playerRef.SetValue(masterScript.MoMVeilIsEquipped, 1.0)
            Else
                playerRef.SetValue(masterScript.MoMVeilIsEquipped, 0.0)
            EndIf
        EndIf
        RefCollectionAlias grids = masterScript.RiversideManorLaserGrids
        If grids != None
            index = 0
            While index < grids.GetCount()
                ObjectReference grid = grids.GetAt(index)
                If grid != None
                    If wearingVeil && !grid.IsDisabled()
                        grid.DisableNoWait()
                    ElseIf !wearingVeil && grid.IsDisabled()
                        grid.EnableNoWait()
                    EndIf
                EndIf
                index += 1
            EndWhile
        EndIf
    EndWhile
    lock_MoMItemEquipState = False
EndFunction

Function NoteLocalEquipEvent(Form akBaseObject)
    If akBaseObject as Armor
        lastArmorEquipEventWasFromEye = akBaseObject == MoM_ClothesMistressOfMysteryEyeOfRa
    EndIf
EndFunction

Function EnforceEyeOfRaRequiresGarb(Actor playerRef, Bool abShowMessage)
    If MoM_ClothesMistressOfMysteryEyeOfRa == None || MoMDressItemKeyword == None
        Return
    EndIf
    If !playerRef.IsEquipped(MoM_ClothesMistressOfMysteryEyeOfRa) || playerRef.WornHasKeyword(MoMDressItemKeyword)
        Return
    EndIf
    playerRef.UnequipItem(MoM_ClothesMistressOfMysteryEyeOfRa, False, True)
    If abShowMessage && MoMEyeOfRaUnequipMessage != None
        Float now = Utility.GetCurrentRealTime()
        ; Real time restarts with each game session, so an older timestamp cannot suppress the message.
        If EyeOfRaNotificationDisplayTimestamp <= 0.0 || now < EyeOfRaNotificationDisplayTimestamp || now - EyeOfRaNotificationDisplayTimestamp >= CONST_EyeOfRaNotificationDelay
            EyeOfRaNotificationDisplayTimestamp = now
            MoMEyeOfRaUnequipMessage.Show()
        EndIf
    EndIf
EndFunction
