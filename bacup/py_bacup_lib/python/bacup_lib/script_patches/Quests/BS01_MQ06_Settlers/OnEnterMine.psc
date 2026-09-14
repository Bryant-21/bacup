Event OnAliasInit()
    Actor playerRef = GetActorReference()
    If playerRef == Game.GetPlayer()
        ReconcileQuestLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    Actor playerRef = GetActorReference()
    If playerRef != Game.GetPlayer() || Quest_M06 == None
        Return
    EndIf

    ReconcileQuestLocation(akNewLoc)
    If Loc_Mine != None && akNewLoc == Loc_Mine
        playerRef.SetValue(AV_BreadCrumb, 0.0)
    ElseIf Loc_Mine != None && akOldLoc == Loc_Mine && Quest_M06.IsStageDone(Stage_MikeIsDead) && !Quest_M06.IsStageDone(Stage_PlayerHasKey) && !Quest_M06.IsStageDone(StageToSet_LeftEarly)
        Quest_M06.SetStage(StageToSet_LeftEarly)
    EndIf
EndEvent

Event OnPlayerLoadGame()
    Actor playerRef = GetActorReference()
    If playerRef == Game.GetPlayer()
        ReconcileQuestLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Function ReconcileQuestLocation(Location playerLocation)
    If Quest_M06 == None || playerLocation == None
        Return
    EndIf

    LocationAlias supplyRoomAlias = Quest_M06.GetAlias(44) as LocationAlias
    If supplyRoomAlias != None && playerLocation == supplyRoomAlias.GetLocation() && Quest_M06.IsStageDone(200) && !Quest_M06.IsStageDone(300)
        Quest_M06.SetStage(300)
    EndIf
    If Loc_Mine != None && playerLocation == Loc_Mine && Quest_M06.IsStageDone(450) && !Quest_M06.IsStageDone(625)
        Quest_M06.SetStage(625)
    EndIf
EndFunction
