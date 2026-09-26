Event OnQuestInit()
    B21PartyCrasherRolled = False
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == questCompleteStage
        RollPartyCrasher()
    EndIf
EndEvent

; A successful event may be crashed once per run by one creature from the list.
Function RollPartyCrasher()
    If B21PartyCrasherRolled || SpawnList == None || SpawnList.Length == 0
        Return
    EndIf
    B21PartyCrasherRolled = True
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None && !eventQuest.IsPlayerParticipating()
        Return
    EndIf
    PartyCrashers crasher = SpawnList[Utility.RandomInt(0, SpawnList.Length - 1)]
    If crasher == None || crasher.CreatureToSpawn == None || crasher.SpawnMarker == None || crasher.SpawnChance == None
        Return
    EndIf
    ObjectReference marker = crasher.SpawnMarker.GetReference()
    If marker == None || Utility.RandomFloat(0.0, 1.0) >= crasher.SpawnChance.GetValue()
        Return
    EndIf
    Actor spawned = marker.PlaceActorAtMe(crasher.CreatureToSpawn)
    If spawned == None
        Return
    EndIf
    ApplyPartyCrasherRank(spawned, crasher.EpicRank)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetDistance(marker) <= NearbyPlayerRange as Float
        If crasher.SpawnSoundEffect != None
            crasher.SpawnSoundEffect.Play(playerRef)
        EndIf
        If PartyCrasherSpawnMessage != None
            PartyCrasherSpawnMessage.Show()
        EndIf
    EndIf
EndFunction

; FO76 epic ranks become the Tales LegendaryStars rank when Tales is installed.
Function ApplyPartyCrasherRank(Actor akActor, Int aiRank)
    If aiRank <= 0 || !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return
    EndIf
    ActorValue rankValue = Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
    If rankValue == None
        Return
    EndIf
    If aiRank > 5
        aiRank = 5
    EndIf
    akActor.SetValue(rankValue, aiRank as Float)
EndFunction
