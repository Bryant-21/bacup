; Stage 60 already starts wave one in the FO4 substitute; the controller ignores
; a repeated request, so the radio line ending still routes through it.
Function Fragment_End()
    ENz04_BotScript controller = GetOwningQuest() as ENz04_BotScript
    If controller != None
        controller.ENz04_StartHostileWave(0)
    EndIf
EndFunction
