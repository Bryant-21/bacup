Event OnQuestInit()
    PlayerRef = Alias_Player.GetActorReference()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
        Alias_Player.ForceRefIfEmpty(PlayerRef)
    EndIf

    BigFish_QuestInstance = Self
    If PlayerRef != None
        PlayerRef.SetValue(FishCaughtAV, 0.0)
        PlayerRef.SetValue(ChosenRegionAV, -1.0)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != 200 || RegionStages == None || RegionStages.Length == 0
        Return
    EndIf

    Int regionIndex = Utility.RandomInt(0, RegionStages.Length - 1)
    If PlayerRef != None
        PlayerRef.SetValue(ChosenRegionAV, regionIndex as Float)
    EndIf
    SetStage(RegionStages[regionIndex])
EndEvent
