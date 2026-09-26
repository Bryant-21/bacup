Event OnQuestInit()
    B21WeatherOverridden = False
    RefreshWeatherOverride()
    StartTimer(2.0, 7632)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshWeatherOverride()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7632 && IsRunning()
        RefreshWeatherOverride()
        StartTimer(2.0, 7632)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7632)
    ReleaseWeatherOverride()
EndEvent

Function SetRegionToAffect(Form newRegion)
    RegionToAffect = newRegion
    If B21WeatherOverridden
        ReleaseWeatherOverride()
    EndIf
    RefreshWeatherOverride()
EndFunction

; FO4 weather overrides are worldspace-wide, so the event area stands in for the FO76 region.
Bool Function WeatherOverrideWanted()
    If DesiredWeather == None || !IsRunning()
        Return False
    EndIf
    If StageToStartWeather >= 0 && !IsStageDone(StageToStartWeather)
        Return False
    EndIf
    If StageToStopWeather >= 0 && IsStageDone(StageToStopWeather)
        Return False
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || playerRef.IsInInterior()
        Return False
    EndIf
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function RefreshWeatherOverride()
    If WeatherOverrideWanted()
        If !B21WeatherOverridden
            DesiredWeather.SetActive(True, abAccelerate)
            B21WeatherOverridden = True
        EndIf
    ElseIf B21WeatherOverridden
        ReleaseWeatherOverride()
    EndIf
EndFunction

Function ReleaseWeatherOverride()
    If B21WeatherOverridden
        B21WeatherOverridden = False
        Weather.ReleaseOverride()
    EndIf
EndFunction
