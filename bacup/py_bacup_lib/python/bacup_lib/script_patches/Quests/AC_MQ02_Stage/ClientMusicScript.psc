Function PlaySoundOnClient(Int trackIndex)
    StopSoundOnClient()
    If TrackList != None && trackIndex >= 0 && trackIndex < TrackList.Length && TrackList[trackIndex] != None
        playbackID = TrackList[trackIndex].Play(Self as ObjectReference)
    EndIf
EndFunction

Function StopSoundOnClient()
    If playbackID > 0
        Sound.StopInstance(playbackID)
        playbackID = 0
    EndIf
EndFunction

Event OnUnload()
    StopSoundOnClient()
EndEvent
