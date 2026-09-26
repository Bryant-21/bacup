Event OnQuestInit()
    FirstInvasionWaveSpawned = False
    Quest owner = Self as Quest
    EWS = owner as DefaultQuestEncounterWaveScript
    If EWS != None && IsTakeoverActive()
        RegisterForCustomEvent(EWS, "FirstSubwaveSpawned")
    EndIf
EndEvent

Event OnQuestShutdown()
    If FirstInvasionWaveSpawned && Worlds_WeatherNukaQuantumStorm != None && Weather.GetCurrentWeather() == Worlds_WeatherNukaQuantumStorm
        Weather.ReleaseOverride()
    EndIf
    FirstInvasionWaveSpawned = False
EndEvent

; LCP_E07B_Invaders is the live-content switch for the Invaders from Beyond season; it ships at 0.
Bool Function IsTakeoverActive()
    Return LCP_E07B_Invaders != None && LCP_E07B_Invaders.GetValue() > 0.0 && EncounterWaveIndices != None
EndFunction

Int Function GetTakeoverWaveIndex(Int aiRegularIndex)
    If !IsTakeoverActive()
        Return aiRegularIndex
    EndIf
    Int index = 0
    While index < EncounterWaveIndices.Length
        WaveIndices row = EncounterWaveIndices[index]
        If row != None && row.RegularEWSIndex == aiRegularIndex && row.AlienEWSIndex >= 0
            Return row.AlienEWSIndex
        EndIf
        index += 1
    EndWhile
    Return aiRegularIndex
EndFunction

Bool Function IsAlienWave(Int aiWaveIndex)
    Int index = 0
    While EncounterWaveIndices != None && index < EncounterWaveIndices.Length
        If EncounterWaveIndices[index] != None && EncounterWaveIndices[index].AlienEWSIndex == aiWaveIndex
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

; Event fragments start their regular waves; during a takeover each one is replaced by its alien round.
Event DefaultQuestEncounterWaveScript.FirstSubwaveSpawned(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If akSender != EWS || akArgs == None || akArgs.Length == 0 || !IsTakeoverActive()
        Return
    EndIf
    Int waveIndex = akArgs[0] as Int
    If IsAlienWave(waveIndex)
        BeginInvasionEffects()
        Return
    EndIf
    Int alienIndex = GetTakeoverWaveIndex(waveIndex)
    If alienIndex != waveIndex
        EWS.StopEncounterWave(waveIndex, True)
        EWS.StartEncounterWave(alienIndex)
    EndIf
EndEvent

Function BeginInvasionEffects()
    If FirstInvasionWaveSpawned
        Return
    EndIf
    FirstInvasionWaveSpawned = True
    If E07B_Invaders_Message_PublicEventInvasion != None
        E07B_Invaders_Message_PublicEventInvasion.Show()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If ZetanAudioStinger != None && playerRef != None
        ZetanAudioStinger.Play(playerRef)
    EndIf
    If Worlds_WeatherNukaQuantumStorm != None && playerRef != None && !playerRef.IsInInterior()
        Worlds_WeatherNukaQuantumStorm.SetActive(True, True)
    EndIf
EndFunction
