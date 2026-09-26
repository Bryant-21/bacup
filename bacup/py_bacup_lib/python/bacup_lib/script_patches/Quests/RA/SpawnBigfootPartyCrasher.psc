Event OnQuestInit()
    B21PartyCrasherRolled = False
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == questCompleteStage
        RollPartyCrasher()
    EndIf
EndEvent

; A successful event may be crashed once per run by the forest creature.
Function RollPartyCrasher()
    If B21PartyCrasherRolled
        Return
    EndIf
    B21PartyCrasherRolled = True
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None && !eventQuest.IsPlayerParticipating()
        Return
    EndIf
    If LvlBigfoot_PartyCrasher == None || spawnMarker == None || RA_PartyCrasherSpawnChance_Bigfoot == None
        Return
    EndIf
    ObjectReference marker = spawnMarker.GetReference()
    If marker == None || Utility.RandomFloat(0.0, 1.0) >= RA_PartyCrasherSpawnChance_Bigfoot.GetValue()
        Return
    EndIf
    If marker.PlaceActorAtMe(LvlBigfoot_PartyCrasher) == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetDistance(marker) <= NearbyPlayerRange as Float
        If MUSPartyCrashersBigfoot_01Start != None
            MUSPartyCrashersBigfoot_01Start.Play(playerRef)
        EndIf
        If PartyCrasherSpawnMessage_Bigfoot != None
            PartyCrasherSpawnMessage_Bigfoot.Show()
        EndIf
    EndIf
EndFunction
