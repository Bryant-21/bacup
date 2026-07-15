; FO4's Sound.Play is usable, but the in-process compiler currently loses its
; integer return type for this FO76-derived script. Start the music without
; assigning the unresolved result so the race remains audible and compiles.

Function PlayMainLoopSFX()
    If MusicLoop != None
        MusicLoop.Play(Self)
    EndIf
    mainLoopInstanceID = 0
EndFunction

Function PlayMainLoopFastSFX()
    If MusicFastLoop != None
        MusicFastLoop.Play(Self)
    EndIf
    mainLoopInstanceID = 0
EndFunction
